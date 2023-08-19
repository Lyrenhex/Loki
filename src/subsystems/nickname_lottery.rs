use std::{collections::HashMap, str::FromStr, time::Duration};

use log::{error, info};
use rand::{
    distributions::Distribution,
    seq::{IteratorRandom, SliceRandom},
};
use serde::{Deserialize, Serialize};
use serenity::{
    async_trait,
    futures::StreamExt,
    model::{
        prelude::{interaction::application_command::CommandDataOptionValue, Guild, UserId},
        Permissions,
    },
    prelude::Context,
};

#[cfg(feature = "events")]
use crate::{command::notify_subscribers, subsystems::events::Event};

use crate::{command::OptionType, config::Config};
use crate::{
    command::{Command, PermissionType},
    get_guild,
};

use super::Subsystem;

#[derive(Default)]
pub struct NicknameLottery;

#[derive(Serialize, Deserialize, Default)]
pub struct NicknameLotteryGuildData {
    /// HashMap of stringified [UserId]s to their respective list of specific nicknames, or [None] if they are excluded from the system.
    user_specific_nicknames: HashMap<String, Vec<String>>,
}

impl NicknameLotteryGuildData {
    /// Deconstructs a list of nicknames, separated by newlines, to into a [Vec].
    pub fn deconstruct_nickname_string(nicks: &str) -> Vec<String> {
        nicks
            .split('\n')
            .filter_map(|n| {
                let n = n.trim();
                if n.is_empty() {
                    return None;
                }
                Some(n.to_string().chars().take(30).collect())
            })
            .collect()
    }

    /// Constructs a [String] listing the nicknames separated by newlines.
    pub fn construct_nickname_string(nicks: &[String]) -> String {
        nicks.join("\n")
    }

    /// Returns the list of specific nicknames for a given [UserId] as a [String], with each entry on a separate line.
    pub fn user_nicknames_string(&self, user: &UserId) -> String {
        self.user_specific_nicknames
            .get(&user.to_string())
            .map_or(String::default(), |n| Self::construct_nickname_string(n))
    }

    /// Sets the list of specific nicknames based for a given [UserId].
    pub fn set_user_nicknames(&mut self, user: &UserId, nicks: &[&str]) {
        if nicks.is_empty() {
            self.user_specific_nicknames.remove(&user.to_string());
        } else {
            self.user_specific_nicknames.insert(
                user.to_string(),
                nicks.iter().map(|s| s.to_string()).collect(),
            );
        }
    }

    /// Select a nickname for the given [UserId], or [None] if the user is excluded.
    pub fn get_nickname_for_user(&self, user: &UserId) -> Option<String> {
        self.user_specific_nicknames
            .get(&user.to_string())
            .map(|n| n.choose(&mut rand::thread_rng()))
            .unwrap_or_default()
            .map(|s| s.to_string())
    }

    /// Select a [UserId] to change the nickname of.
    pub fn get_random_user(&self) -> Option<UserId> {
        self.user_specific_nicknames
            .keys()
            .choose(&mut rand::thread_rng())
            .map(|id| UserId(u64::from_str(id).unwrap()))
    }
}

#[async_trait]
impl Subsystem for NicknameLottery {
    fn generate_commands(&self) -> Vec<Command<'static>> {
        vec![
            Command::new(
                "nickname_lottery",
                "Controls for the nickname lottery.",
                PermissionType::ServerPerms(Permissions::MANAGE_NICKNAMES),
                None,
            )
            .add_variant(
                Command::new(
                    "set_nicknames",
                    "Set the list of nicknames that can be applied, either generally or to a specific user.",
                    PermissionType::ServerPerms(Permissions::MANAGE_NICKNAMES),
                    Some(Box::new(move |ctx, command| {
                        Box::pin(async move {
                            let user = command.data.options[0].options.iter().find(|opt| opt.name == "user").and_then(|u| {
                                if let Some(CommandDataOptionValue::User(user, _)) = &u.resolved {
                                    Some(user.clone())
                                } else {
                                    None
                                }
                            }).unwrap();
                            let guild_id = command.guild_id.unwrap();

                            let data = ctx.data.read().await;
                            let guild = get_guild(&data, &guild_id).unwrap();
                            let nickname_lottery_data = guild.nickname_lottery_data();

                            let old_nicknames = nickname_lottery_data.user_nicknames_string(&user.id);

                            let mut input_nicks = serenity::builder::CreateInputText::default();
                            input_nicks
                                .label("Nicknames list")
                                .custom_id("nicknames_list")
                                .style(serenity::model::prelude::component::InputTextStyle::Paragraph)
                                .placeholder("List of nicknames, each on a new line (or blank to unset).")
                                .required(false)
                                .value(old_nicknames);
                            drop(data);

                            let mut components = serenity::builder::CreateComponents::default();
                            components.create_action_row(|r| r.add_input_text(input_nicks));

                            command
                                .create_interaction_response(&ctx.http, |r| {
                                    r.kind(
                                        serenity::model::application::interaction::InteractionResponseType::Modal,
                                    );
                                    r.interaction_response_data(|d| {
                                        d.title(format!("{}'s nicknames",
                                            user.name
                                        ))
                                        .custom_id(user.id.to_string() + "_set_nicknames")
                                        .set_components(components)
                                    })
                                })
                                .await?;

                            let userid = user.id;
                            // collect the submitted data
                            let collector =
                                serenity::collector::ModalInteractionCollectorBuilder::new(ctx)
                                    .filter(move |int| int.data.custom_id == userid.to_string() + "_set_nicknames")
                                    .collect_limit(1)
                                    .build();

                            collector.then(|int| async move {
                                let mut data = ctx.data.write().await;
                                let config = data.get_mut::<Config>().unwrap();
                                let guild = config.guild_mut(&guild_id.clone());
                                let nickname_lottery_data = guild.nickname_lottery_data_mut();

                                let inputs: Vec<_> = int
                                    .data
                                    .components
                                    .iter()
                                    .flat_map(|r| r.components.iter())
                                    .collect();

                                for input in inputs.iter() {
                                    if let serenity::model::prelude::component::ActionRowComponent::InputText(it) = input {
                                        if it.custom_id == "nicknames_list" {
                                                nickname_lottery_data.set_user_nicknames(&user.id, &NicknameLotteryGuildData::deconstruct_nickname_string(&it.value.clone()).iter().map(|n| n.as_str()).collect::<Vec<&str>>());
                                        }
                                    }
                                }

                                config.save();

                                // it's now safe to close the modal, so send a response to it
                                int.create_interaction_response(&ctx.http, |r| {
                                    r.kind(serenity::model::prelude::interaction::InteractionResponseType::DeferredUpdateMessage)
                                })
                                .await
                                .ok();
                            })
                            .collect::<Vec<_>>()
                            .await;

                            Ok(())
                        })
                    })),
                )
                .add_option(
                    crate::Option::new(
                        "user",
                        "The user to set the nickname list for.",
                        OptionType::User,
                        true
                    )
                ),
            )
        ]
    }
}

impl NicknameLottery {
    pub async fn guild_init(ctx: Context, g: Guild) {
        // between 30 minutes and 7 days
        let between = rand::distributions::Uniform::from(1_800..604_800);
        loop {
            if cfg!(not(debug_assertions)) {
                let tts = Duration::from_secs(between.sample(&mut rand::thread_rng()));
                info!(
                    "[Guild: {}] Next nickname change in {} minutes.",
                    g.id,
                    (tts.as_secs() / 60)
                );
                tokio::time::sleep(tts).await;
            } else {
                info!(
                    "[Guild: {}] Running nickname lottery immediately once, for debugging.",
                    g.id
                );
            }
            // Time to update a user's nickname!
            let data = ctx.data.read().await;
            if let Some(guild) = get_guild(&data, &g.id) {
                let lottery_data = guild.nickname_lottery_data();
                if let Some(user) = lottery_data.get_random_user() {
                    if let Ok(member) = g.member(&ctx.http, user).await {
                        let user = &member.user;
                        if let Some(mut new_nick) = lottery_data.get_nickname_for_user(&user.id) {
                            let old_nick = member.display_name();
                            // If feature `stream-indicator` is enabled, we want to preserve any applied streaming prefix, in case we're changing the nickname mid-stream.
                            #[cfg(feature = "stream-indicator")]
                            if old_nick
                                .starts_with(crate::subsystems::stream_indicator::STREAMING_PREFIX)
                            {
                                new_nick = crate::subsystems::stream_indicator::STREAMING_PREFIX
                                    .to_string()
                                    + &new_nick;
                            }
                            info!(
                                "[Guild: {}] Updating {}'s nickname to {new_nick} (current: {})",
                                &g.id, &user.id, &old_nick
                            );
                            if let Err(e) = g
                                .edit_member(&ctx.http, user.id, |m| m.nickname(new_nick))
                                .await
                            {
                                #[cfg(feature = "events")]
                                notify_subscribers(
                                    &ctx,
                                    Event::Error,
                                    &format!(
                                        "**[Guild: {}] Error changing {}'s nickname:**
{e}",
                                        g.id, user.id
                                    ),
                                )
                                .await;
                                error!(
                                    "[Guild: {}] Error changing {}'s nickname:
{e}",
                                    g.id, user.id
                                );
                            }
                        }
                    }
                }
            }
            if cfg!(debug_assertions) {
                break;
            }
        }
    }
}

#[cfg(test)]
mod test {
    use serenity::model::prelude::UserId;

    use super::NicknameLotteryGuildData;

    #[test]
    fn test_nickname_strings() {
        assert_eq!(
            NicknameLotteryGuildData::construct_nickname_string(
                &NicknameLotteryGuildData::deconstruct_nickname_string(
                    &"Nick1
Nick number 2    
     Nick_numerus_tres

A very long nickname that exceeds 30 characters
etc"
                )
            ),
            "Nick1
Nick number 2
Nick_numerus_tres
A very long nickname that exce
etc"
        )
    }

    #[test]
    fn test_setting_and_selecting_nicknames() {
        let users = [UserId::from(0), UserId::from(1)];
        let mut data: NicknameLotteryGuildData = NicknameLotteryGuildData::default();
        assert_eq!(data.get_nickname_for_user(&users[0]), None);
        assert_eq!(data.get_nickname_for_user(&users[1]), None);
        data.set_user_nicknames(&users[0], &["user0"]);
        data.set_user_nicknames(&users[1], &["user1"]);
        assert_eq!(
            data.get_nickname_for_user(&users[0]),
            Some("user0".to_string())
        );
        assert_eq!(
            data.get_nickname_for_user(&users[1]),
            Some("user1".to_string())
        );
        data.set_user_nicknames(&users[0], &[]);
        assert_eq!(data.get_nickname_for_user(&users[0]), None);
        assert_eq!(
            data.get_nickname_for_user(&users[1]),
            Some("user1".to_string())
        );
    }

    #[test]
    fn select_random_user() {
        let users = [UserId::from(0)];
        let mut data: NicknameLotteryGuildData = NicknameLotteryGuildData::default();
        assert_eq!(data.get_random_user(), None);
        data.set_user_nicknames(&users[0], &["user0"]);
        assert_eq!(data.get_random_user(), Some(users[0].clone()));
        data.set_user_nicknames(&users[0], &[]);
        assert_eq!(data.get_random_user(), None);
    }
}
