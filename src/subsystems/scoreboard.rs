use std::collections::HashMap;

use const_format::formatcp;
use log::{error, info, trace};
use serde::{Deserialize, Serialize};
use serenity::{
    all::Mentionable as _,
    async_trait, futures,
    model::{
        gateway::Ready,
        guild::Guild,
        id::{CommandId, GuildId, UserId},
        Permissions,
    },
    prelude::Context,
};
use tinyvec::ArrayVec;

use crate::{
    command::{Command, OptionType, PermissionType},
    config::{get_guild, Config},
    create_raw_embed, ActionResponse, NUM_SELECTABLES,
};
#[cfg(feature = "events")]
use crate::{notify_subscribers, subsystems::events::Event};

use super::Subsystem;

pub const NUM_SCOREBOARDS: usize = crate::command::NUM_SELECTABLES - 1;

#[derive(Serialize, Deserialize, Default, Clone)]
pub struct Scoreboard {
    /// [HashMap] from each UserId (as String) to their respective score.
    scores: HashMap<String, i64>,
}

impl Scoreboard {
    pub fn new() -> Self {
        Self {
            scores: HashMap::new(),
        }
    }

    pub fn set_user(&mut self, user: &UserId, score: i64) -> Option<i64> {
        self.scores.insert(user.to_string(), score)
    }

    fn _scores(&self) -> Vec<(usize, UserId, i64)> {
        let mut entries = self
            .scores
            .iter()
            .filter_map(|(uid, count)| uid.parse::<u64>().ok().map(|uid| (uid, count)))
            .map(|(uid, count)| (uid.into(), *count))
            .collect::<Vec<(UserId, i64)>>();
        entries.sort_unstable_by(|(_, cnt_a), (_, cnt_b)| cnt_b.cmp(cnt_a));
        entries
            .into_iter()
            .enumerate()
            .map(|(i, (uid, cnt))| (i + 1, uid, cnt))
            .collect::<Vec<(usize, UserId, i64)>>()
    }

    pub fn scores(&self) -> Vec<(usize, UserId, i64)> {
        self._scores().into_iter().take(10).collect()
    }

    pub fn score(&self, user: &UserId) -> Option<(usize, UserId, i64)> {
        self._scores().into_iter().find(|(_, uid, _)| uid == user)
    }
}

#[derive(Serialize, Deserialize, Default, Clone)]
pub struct ScoreboardData {
    scoreboards: HashMap<String, Scoreboard>,
    ephemeral_command_id: Option<CommandId>,
}

impl ScoreboardData {
    pub async fn set_ephemeral_commands(
        &mut self,
        ctx: &Context,
        g: &GuildId,
    ) -> crate::Result<()> {
        if self.scoreboards.is_empty() {
            if let Some(cid) = self.ephemeral_command_id {
                self.ephemeral_command_id = None;
                g.delete_command(&ctx, cid).await?;
                info!(
                    "[Guild: {}] Deleted ephemeral `scoreboard` command (id {cid})",
                    g
                );
            }
            return Ok(());
        }
        let scoreboard_select = crate::command::Option::new(
            "name",
            "Which scoreboard to use.",
            OptionType::StringSelect(Box::new({
                let mut v = self
                    .scoreboards
                    .keys()
                    .take(NUM_SCOREBOARDS)
                    .cloned()
                    .collect::<ArrayVec<[String; NUM_SELECTABLES]>>();
                v.sort();
                v
            })),
            true,
        );
        let command = Command::new(
            "scoreboard",
            "Track all the scores!",
            PermissionType::ServerPerms(Permissions::USE_APPLICATION_COMMANDS),
            None,
        )
        .add_variant(
            Command::new(
                "delete",
                "Delete a scoreboard.",
                PermissionType::ServerPerms(Permissions::ADMINISTRATOR),
                None,
            )
            .add_option(scoreboard_select.clone()),
        )
        .add_variant(
            Command::new(
                "view",
                "View the top 10 scores on the board, or a given user's score.",
                PermissionType::ServerPerms(Permissions::USE_APPLICATION_COMMANDS),
                None,
            )
            .add_option(scoreboard_select.clone())
            .add_option(crate::command::Option::new(
                "user",
                "The specific user to check the score of.",
                OptionType::User,
                false,
            )),
        )
        .add_variant(
            Command::new(
                "set",
                "Set your score on a board.",
                PermissionType::ServerPerms(Permissions::USE_APPLICATION_COMMANDS),
                None,
            )
            .add_option(scoreboard_select.clone())
            .add_option(crate::command::Option::new(
                "score",
                "Your score!",
                OptionType::IntegerInput(None, None),
                true,
            )),
        )
        .add_variant(
            Command::new(
                "override",
                "Override a user's score on the board.",
                PermissionType::ServerPerms(Permissions::ADMINISTRATOR),
                None,
            )
            .add_option(scoreboard_select.clone())
            .add_option(crate::command::Option::new(
                "user",
                "The user whose score you wish to override.",
                OptionType::User,
                true,
            ))
            .add_option(crate::command::Option::new(
                "score",
                "The score to set for the given user.",
                OptionType::IntegerInput(None, None),
                true,
            )),
        );
        self.ephemeral_command_id = Some(
            g.create_command(&ctx, crate::serenity_handler::construct_command(&command))
                .await?
                .id,
        );
        info!(
            "[Guild: {}] Created ephemeral `scoreboard` command (id {}) with {} variants",
            g,
            self.ephemeral_command_id.unwrap(),
            self.scoreboards.len()
        );
        Ok(())
    }

    pub async fn add_scoreboard(
        &mut self,
        name: &String,
        ctx: &Context,
        g: &GuildId,
    ) -> Result<Result<(), &str>, crate::Error> {
        if self.scoreboards.len() >= NUM_SCOREBOARDS {
            return Ok(Err(
                "The maximum number of scoreboards already exist - consider deleting one.",
            ));
        }
        if self.scoreboards.contains_key(name) {
            return Ok(Err("A scoreboard with that name already exists."));
        }
        self.scoreboards.insert(name.clone(), Scoreboard::new());
        self.set_ephemeral_commands(ctx, g).await?;
        Ok(Ok(()))
    }

    pub fn scoreboards(&self) -> Vec<(&String, &Scoreboard)> {
        self.scoreboards
            .iter()
            .collect::<Vec<(&String, &Scoreboard)>>()
    }

    pub fn scoreboard(&self, name: &String) -> Option<&Scoreboard> {
        self.scoreboards.get(name)
    }

    pub fn update_scoreboard(
        &mut self,
        name: &String,
        user: &UserId,
        score: i64,
    ) -> crate::Result<Option<i64>> {
        if let Some(sb) = self.scoreboards.get_mut(name) {
            Ok(sb.set_user(user, score))
        } else {
            Err(crate::Error::InvalidParam(format!(
                "Scoreboard {name} does not exist."
            )))
        }
    }

    pub async fn delete_scoreboard(
        &mut self,
        name: &String,
        ctx: &Context,
        g: &GuildId,
    ) -> crate::Result<()> {
        self.scoreboards.remove(name);
        self.set_ephemeral_commands(ctx, g).await?;
        Ok(())
    }
}

#[derive(Serialize, Deserialize, Default, Clone)]
pub struct Scoreboards;

#[async_trait]
impl Subsystem for Scoreboards {
    fn generate_commands(&self) -> Vec<crate::command::Command<'static>> {
        vec![
            Command::new(
                "create_scoreboard",
                formatcp!("Create a new scoreboard (max. {NUM_SCOREBOARDS})."),
                PermissionType::ServerPerms(Permissions::ADMINISTRATOR),
                Some(Box::new(move |ctx, command, params| {
                    Box::pin(async {
                        let name = get_param!(params, String, "name");
                        let mut data = crate::acquire_data_handle!(write ctx);
                        let config = data.get_mut::<Config>().unwrap();
                        let guild = config.guild_mut(&command.guild_id.unwrap());
                        let resp = if let Err(e) = guild
                            .scoreboards_mut()
                            .add_scoreboard(name, ctx, &command.guild_id.unwrap())
                            .await?
                        {
                            format!(
                                "**Could not create scoreboard `{name}`:**
        {e}"
                            )
                        } else {
                            config.save();
                            format!("**Created new scoreboard `{name}`!**")
                        };
                        crate::drop_data_handle!(data);
                        Ok(Some(ActionResponse::new(create_raw_embed(resp), false)))
                    })
                })),
            )
            .add_option(crate::command::Option::new(
                "name",
                "The scoreboard's name.",
                OptionType::StringInput(Some(1), None),
                true,
            )),
            Command::new_stub("scoreboard", None)
                .add_variant(Command::new_stub(
                    "delete",
                    Some(Box::new(move |ctx, command, params| {
                        Box::pin(async {
                            let name = get_param!(params, String, "name");
                            let mut data = crate::acquire_data_handle!(write ctx);
                            let config = data.get_mut::<Config>().unwrap();
                            let guild = config.guild_mut(&command.guild_id.unwrap());
                            guild
                                .scoreboards_mut()
                                .delete_scoreboard(name, ctx, &command.guild_id.unwrap())
                                .await?;
                            config.save();
                            crate::drop_data_handle!(data);
                            let resp = format!("**Deleted scoreboard `{name}`.**");
                            Ok(Some(ActionResponse::new(create_raw_embed(resp), false)))
                        })
                    })),
                ))
                .add_variant(Command::new_stub(
                    "view",
                    Some(Box::new(move |ctx, command, params| {
                        Box::pin(async move {
                            let name = get_param!(params, String, "name");
                            let mut positions = String::new();
                            let mut users = String::new();
                            let mut scores = String::new();
                            let data = crate::acquire_data_handle!(read ctx);
                            if let Some(guild) = get_guild(&data, &command.guild_id.unwrap()) {
                                let scoreboard = guild.scoreboards().scoreboard(name).ok_or(
                                    crate::Error::InvalidParam(format!(
                                        "Scoreboard {name} does not exist!"
                                    )),
                                )?;
                                if params.len() > 1 {
                                    let user = get_param!(params, User, "user");
                                    let user = command.data.resolved.users.get(user).unwrap();
                                    if let Some((p, _, s)) = scoreboard.score(&user.id) {
                                        positions = p.to_string();
                                        users = user.mention().to_string();
                                        scores = s.to_string();
                                    }
                                } else {
                                    let entries = scoreboard.scores();
                                    positions = entries
                                        .iter()
                                        .map(|(p, _, _)| p.to_string())
                                        .collect::<Vec<String>>()
                                        .join("\n");
                                    users = futures::future::try_join_all(entries.iter().map(
                                        |(_, uid, _)| async {
                                            Ok::<String, crate::Error>(
                                                uid.to_user(&ctx).await?.mention().to_string(),
                                            )
                                        },
                                    ))
                                    .await?
                                    .join("\n");
                                    scores = entries
                                        .iter()
                                        .map(|(_, _, cnt)| cnt.to_string())
                                        .collect::<Vec<String>>()
                                        .join("\n");
                                }
                            }
                            let resp = create_raw_embed(format!("**{name}**"))
                                .field("#", positions, true)
                                .field("User", users, true)
                                .field("Score", scores, true);
                            Ok(Some(ActionResponse::new(resp, false)))
                        })
                    })),
                ))
                .add_variant(Command::new_stub(
                    "set",
                    Some(Box::new(move |ctx, command, params| {
                        Box::pin(async {
                            let name = get_param!(params, String, "name");
                            let score = *get_param!(params, Integer, "score");
                            let mut data = crate::acquire_data_handle!(write ctx);
                            let config = data.get_mut::<Config>().unwrap();
                            let guild = config.guild_mut(&command.guild_id.unwrap());
                            let prev = guild.scoreboards_mut().update_scoreboard(
                                name,
                                &command.user.id,
                                score,
                            )?;
                            config.save();
                            crate::drop_data_handle!(data);
                            let resp = format!(
                                "**Updated scoreboard `{name}`**
        {} has updated their score to `{score}`{}.",
                                command.user.mention(),
                                if let Some(prev) = prev {
                                    format!(" (was `{prev}`)")
                                } else {
                                    String::new()
                                }
                            );
                            Ok(Some(ActionResponse::new(create_raw_embed(resp), false)))
                        })
                    })),
                ))
                .add_variant(Command::new_stub(
                    "override",
                    Some(Box::new(move |ctx, command, params| {
                        Box::pin(async {
                            let name = get_param!(params, String, "name");
                            let user = get_param!(params, User, "user");
                            let user = command.data.resolved.users.get(user).unwrap();
                            let score = *get_param!(params, Integer, "score");

                            let mut data = crate::acquire_data_handle!(write ctx);
                            let config = data.get_mut::<Config>().unwrap();
                            let guild = config.guild_mut(&command.guild_id.unwrap());
                            let prev = guild
                                .scoreboards_mut()
                                .update_scoreboard(name, &user.id, score)?;
                            config.save();
                            crate::drop_data_handle!(data);
                            let resp = format!(
                                "**Updated scoreboard `{name}`**
        {} has overridden {}'s score to `{score}`{}.",
                                command.user.mention(),
                                user.mention(),
                                if let Some(prev) = prev {
                                    format!(" (was `{prev}`)")
                                } else {
                                    String::new()
                                }
                            );
                            Ok(Some(ActionResponse::new(create_raw_embed(resp), false)))
                        })
                    })),
                )),
        ]
    }

    async fn ready(&self, _ctx: &Context, _ready: &Ready) {}
}

impl Scoreboards {
    pub async fn guild_init(ctx: Context, g: Guild) {
        trace!("[Guild: {}] Setting ephemeral `scoreboard` command", g.id);
        let mut data = crate::acquire_data_handle!(write ctx);
        let config = data.get_mut::<Config>().unwrap();
        let guild = config.guild_mut(&g.id);
        if let Err(e) = guild
            .scoreboards_mut()
            .set_ephemeral_commands(&ctx, &g.id)
            .await
        {
            error!(
                "[Guild: {}] Error setting ephemeral `scoreboard` command:
{e}",
                g.id
            );
            #[cfg(feature = "events")]
            notify_subscribers(
                &ctx,
                Event::Error,
                &format!(
                    "**[Guild: {}] Error setting ephemeral `scoreboard` command:**
{e}",
                    g.id
                ),
            )
            .await;
        } else {
            config.save();
        };
        crate::drop_data_handle!(data);
    }
}
