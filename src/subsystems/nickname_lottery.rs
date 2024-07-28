use std::{
    collections::{hash_map::Entry, HashMap},
    str::FromStr,
    time::Duration,
};

use chrono::{DateTime, Datelike, TimeZone, Utc};
use log::{error, info, trace, warn};
use rand::{
    distributions::Distribution,
    seq::{IteratorRandom, SliceRandom},
};
use serde::{Deserialize, Serialize};
use serenity::{
    all::{CacheHttp as _, CommandDataOptionValue, CreateModal, Guild, Mentionable as _, UserId},
    async_trait,
    model::{channel::ChannelType, id::ChannelId, Permissions},
    prelude::Context,
};

#[cfg(feature = "events")]
use crate::{command::notify_subscribers, subsystems::events::Event};

use crate::{
    command::OptionType, config::Config, create_embed, create_raw_embed,
    notify_subscribers_with_handle, ActionResponse,
};
use crate::{
    command::{Command, PermissionType},
    get_guild,
};

use super::Subsystem;

/// (30 mins, 5 days) in seconds.
const DEFAULT_REFRESH_INTERVAL: (u64, u64) = (1_800, 432_000);

#[derive(Default)]
pub struct NicknameLottery;

/// A [Guild]'s collective nickname lottery data, including configuration.
#[derive(Serialize, Deserialize, Default)]
pub struct NicknameLotteryGuildData {
    /// HashMap of stringified [UserId]s to their respective list of specific nicknames, or [None] if they are excluded from the system.
    user_specific_nicknames: HashMap<String, Vec<NicknameData>>,
    /// Channel that the bot demands a name change in, if it fails to do so itself, and announces any nickname changes during April Fool's.
    /// The bot will 'silently' fail, and make no announcements, if this is not set.
    channel: Option<ChannelId>,
    /// An override for the title of the bot name change demand. Uses default if [None].
    title_override: Option<String>,
    /// An override for the refresh interval for this guild. Uses [DEFAULT_REFRESH_INTERVAL] if [None].
    refresh_interval: Option<(u64, u64)>,
}

impl NicknameLotteryGuildData {
    /// Returns the list of specific nicknames for a given [UserId], or [None] if the user does not have any.
    pub fn user_nicknames(&self, user: &UserId) -> Option<&Vec<NicknameData>> {
        self.user_specific_nicknames.get(&user.to_string())
    }

    /// Add a [NicknameData] to a [UserId], returning the index of the added nickname.
    pub fn add_user_nickname(&mut self, user: &UserId, nickname: NicknameData) -> usize {
        trace!("Adding nickname for {user:?}: {nickname:?}");
        self.user_specific_nicknames
            .entry(user.to_string())
            .or_default()
            .push(nickname);
        self.user_specific_nicknames
            .get(&user.to_string())
            .unwrap()
            .len()
            - 1
    }

    pub fn set_user_nickname_context(&mut self, user: &UserId, n: usize, context: String) {
        trace!("Adding context for {user:?} nickname #{n}: {context}");
        assert!(n > 0);
        self.user_specific_nicknames
            .entry(user.to_string())
            .and_modify(|nicknames| {
                assert!(n <= nicknames.len());
                nicknames.get_mut(n - 1).unwrap().set_context(context);
            });
    }

    /// Remove the `n`th [NicknameData] from a [UserId].
    pub fn remove_user_nickname(&mut self, user: &UserId, n: usize) {
        trace!("Removing nickname #{n} for {user:?}");
        assert!(n > 0);
        let entry = self
            .user_specific_nicknames
            .entry(user.to_string())
            .and_modify(|nicknames| {
                assert!(n <= nicknames.len());
                nicknames.remove(n - 1);
            });
        if let Entry::Occupied(entry) = entry {
            if entry.get().is_empty() {
                entry.remove();
            }
        }
    }

    /// Select a nickname for the given [UserId], or [None] if the user is excluded.
    pub fn get_nickname_for_user(&self, user: &UserId) -> Option<&String> {
        self.user_specific_nicknames
            .get(&user.to_string())
            .map(|n| n.choose(&mut rand::thread_rng()))
            .unwrap_or_default()
            .map(|s| s.nickname())
    }

    /// Select a [UserId] to change the nickname of.
    pub fn get_random_user(&self) -> Option<UserId> {
        self.user_specific_nicknames
            .keys()
            .choose(&mut rand::thread_rng())
            .map(|id| UserId::new(u64::from_str(id).unwrap()))
    }

    /// Set the channel.
    pub fn set_channel(&mut self, channel: Option<ChannelId>) {
        self.channel = channel;
    }

    /// Get the channel, if set.
    pub fn channel(&self) -> Option<ChannelId> {
        self.channel
    }

    /// Set title override.
    pub fn set_title_override(&mut self, title_override: Option<String>) {
        self.title_override = title_override;
    }

    /// Get title, either [Self::title_override] or the default string.
    pub fn title(&self) -> String {
        if let Some(title) = &self.title_override {
            title.clone()
        } else {
            "Bot demands new nickname".to_string()
        }
    }

    /// Get the refresh interval for this guild.
    pub fn refresh_interval(&self) -> Option<&(u64, u64)> {
        self.refresh_interval.as_ref()
    }

    /// Set a custom refresh interval for this guild, or reset back to the default if [None].
    pub fn set_refresh_interval(&mut self, refresh_interval: Option<(u64, u64)>) {
        self.refresh_interval = refresh_interval;
    }
}

/// Data for a single nickname, including metadata.
#[derive(Serialize, Deserialize, Default, Debug, Clone)]
pub struct NicknameData {
    /// The nickname itself.
    nickname: String,
    /// The user who added this nickname.
    author: Option<UserId>,
    /// The time that this nickname was created.
    time: Option<DateTime<Utc>>,
    /// Context for the nickname, if any.
    context: Option<String>,
}

impl NicknameData {
    /// Create a new nickname with the supplied data. `time` will be set to the current time.
    pub fn new(nickname: String, author: UserId) -> Self {
        Self {
            nickname,
            author: Some(author),
            time: Some(Utc::now()),
            context: None,
        }
    }

    /// Get the actual nickname this [NicknameData] represents.
    pub fn nickname(&self) -> &String {
        &self.nickname
    }

    /// Get the [UserId] that created this nickname.
    /// [None] if this is unknown, such as if the data was migrated from a pre-v0.11 list of nicknames.
    pub fn author(&self) -> Option<&UserId> {
        self.author.as_ref()
    }

    /// Get the time that this nickname was created.
    /// [None] if this is unknown, such as if the data was migrated from a pre-v0.11 list of nicknames.
    pub fn time(&self) -> Option<&DateTime<Utc>> {
        self.time.as_ref()
    }

    /// Get the context behind this nickname, if any was provided.
    /// This is optional, so may return [None].
    pub fn context(&self) -> Option<&String> {
        self.context.as_ref()
    }

    pub fn set_context(&mut self, context: String) {
        self.context = Some(context);
    }
}

#[async_trait]
impl Subsystem for NicknameLottery {
    fn generate_commands(&self) -> Vec<Command<'static>> {
        vec![Command::new(
            "nickname_lottery",
            "Controls for the nickname lottery.",
            PermissionType::ServerPerms(Permissions::USE_APPLICATION_COMMANDS),
            None,
        )
        .add_variant(
            Command::new(
                "user_nicknames",
                "Manage individual users' nicknames.",
                PermissionType::ServerPerms(Permissions::USE_APPLICATION_COMMANDS),
                None,
            )
            .add_variant(
                Command::new(
                    "add",
                    "Add a new nickname for a user.",
                    PermissionType::ServerPerms(Permissions::MANAGE_NICKNAMES),
                    Some(Box::new(move |ctx, command, params| {
                        Box::pin(async move {
                            let user = get_param!(params, User, "user");
                            let user = command.data.resolved.users.get(user).unwrap();
                            let nickname = get_param!(params, String, "nickname").clone();
                            let guild_id = command.guild_id.unwrap();

                            info!(
                                "[Guild: {}] Adding nickname {nickname} for {} ({}) (author: {} ({}))",
                                guild_id, user.name, user.id, command.user.name, command.user.id
                            );

                            let data = crate::acquire_data_handle!(read ctx);
                            let guild = get_guild(&data, &guild_id).unwrap();
                            let nickname_lottery_data = guild.nickname_lottery_data();

                            if nickname_lottery_data.user_nicknames(&user.id).map(|nicknames| nicknames.iter().any(|nd| *nd.nickname() == nickname)).unwrap_or(false) {
                                info!(
                                    "[Guild: {}] Nickname {nickname} for {} ({}) already exists; ignoring.",
                                    guild_id, user.name, user.id
                                );
                                return Ok(Some(ActionResponse::new(
                                        create_raw_embed(format!("Nickname {nickname} already exists for {}.", user.mention())),
                                        true,
                                    )));
                            }
                            crate::drop_data_handle!(data);

                            let nd = NicknameData::new(nickname.clone(), command.user.id);

                            let mut data = crate::acquire_data_handle!(write ctx);
                            let config = data.get_mut::<Config>().unwrap();
                            let guild = config.guild_mut(&guild_id.clone());
                            let nickname_lottery_data = guild.nickname_lottery_data_mut();

                            let n = nickname_lottery_data.add_user_nickname(&user.id, nd);

                            config.save();
                            crate::drop_data_handle!(data);

                            let input_context = serenity::builder::CreateInputText::new(
                                serenity::all::InputTextStyle::Paragraph,
                                "Nickname context",
                                "nickname_context",
                            )
                            .placeholder("Any context about this nickname to offset future forgetfulness.")
                            .required(false);

                            let components =
                                vec![serenity::all::CreateActionRow::InputText(input_context)];

                            command
                                .create_response(
                                    &ctx.http(),
                                    serenity::all::CreateInteractionResponse::Modal(
                                        CreateModal::new(
                                            user.id.to_string() + "_" + &nickname + "_context",
                                            format!("Context for {nickname}"),
                                        )
                                        .components(components),
                                    ),
                                )
                                .await?;

                            let userid = user.id;
                            let nick = nickname.clone();
                            // collect the submitted data
                            if let Some(int) = serenity::collector::ModalInteractionCollector::new(ctx)
                                .filter(move |int| {
                                    int.data.custom_id == userid.to_string() + "_" + &nick + "_context"
                                })
                                .timeout(Duration::new(300, 0))
                                .await
                            {
                                let mut data = crate::acquire_data_handle!(write ctx);
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
                                    if let serenity::all::ActionRowComponent::InputText(it) = input {
                                        if it.custom_id == "nickname_context" {
                                            if let Some(it) = &it.value {
                                                if !it.is_empty() {
                                                    nickname_lottery_data.set_user_nickname_context(&user.id, n + 1, it.to_string());
                                                }
                                            }
                                        }
                                    }
                                }

                                config.save();

                                crate::drop_data_handle!(data);

                                // it's now safe to close the modal, so send a response to it
                                int.create_response(
                                    &ctx.http(),
                                    serenity::all::CreateInteractionResponse::Acknowledge,
                                )
                                .await?;
                            }

                            Ok(None)
                        })
                    })),
                )
                .add_option(crate::Option::new(
                    "user",
                    "The user to add the nickname for.",
                    OptionType::User,
                    true,
                ))
                .add_option(crate::Option::new(
                    "nickname",
                    "The nickname to add for the user.",
                    OptionType::StringInput(Some(1), Some(30)),
                    true,
                )),
            )
            .add_variant(
                Command::new(
                    "remove",
                    "Remove a nickname from a user.",
                    PermissionType::ServerPerms(Permissions::MANAGE_NICKNAMES),
                    Some(Box::new(move |ctx, command, params| {
                        Box::pin(async move {
                            let user = get_param!(params, User, "user");
                            let user = command.data.resolved.users.get(user).unwrap();
                            let n = *get_param!(params, Integer, "number");
                            let guild_id = command.guild_id.unwrap();

                            if n < 1 {
                                return Ok(Some(ActionResponse::new(
                                        create_raw_embed("**`number` must be greater than 0**
Check the user's nickname list for valid numbers to remove!"),
                                        true,
                                    )))
                            }

                            info!(
                                "[Guild: {}] Removing nickname #{n} for {} ({})",
                                guild_id, user.name, user.id,
                            );

                            let data = crate::acquire_data_handle!(read ctx);
                            let guild = get_guild(&data, &guild_id).unwrap();
                            let nickname_lottery_data = guild.nickname_lottery_data();

                            if nickname_lottery_data.user_nicknames(&user.id).map(|nicknames| n as usize > nicknames.len()).unwrap_or(true) {
                                info!(
                                    "[Guild: {}] Nickname #{n} does not exist for {} ({}); ignoring.",
                                    guild_id, user.name, user.id
                                );
                                return Ok(Some(ActionResponse::new(
                                    create_raw_embed(format!("**Nickname #{n} does not exist for {}**
Consider checking their nickname list for valid number to remove.",
                                        user.mention())),
                                    true,
                                )));
                            }
                            let nickname = &nickname_lottery_data.user_nicknames(&user.id).unwrap()[n as usize - 1].clone();
                            crate::drop_data_handle!(data);

                            let mut data = crate::acquire_data_handle!(write ctx);
                            let config = data.get_mut::<Config>().unwrap();
                            let guild = config.guild_mut(&guild_id.clone());
                            let nickname_lottery_data = guild.nickname_lottery_data_mut();

                            nickname_lottery_data.remove_user_nickname(&user.id, n as usize);

                            config.save();

                            crate::drop_data_handle!(data);

                            Ok(Some(ActionResponse::new(
                                create_raw_embed(
                                    format!("**Removed nickname '{}' for {}**
Originally added by {} ({})
**Context:**
{}",
                                    nickname.nickname(), user.mention(),
                                    nickname.author()
                                            .map(|uid| uid.mention().to_string())
                                            .unwrap_or("`user not known`".to_string()),
                                    nickname.time()
                                            .map(|time| format!("<t:{}:F>", time.timestamp().to_string()))
                                            .unwrap_or("`time not known`".to_string()),
                                    nickname.context()
                                            .unwrap_or(&"No context provided.".to_string()),
                                    )
                                ),
                                true,
                            )))
                        })
                    })),
                )
                .add_option(crate::Option::new(
                    "user",
                    "The user to add the nickname for.",
                    OptionType::User,
                    true,
                ))
                .add_option(crate::Option::new(
                    "number",
                    "The number of the nickname to remove, as reported in the user's nickname list.",
                    OptionType::IntegerInput(Some(1), None),
                    true,
                )),
            )
            .add_variant(
                Command::new(
                    "set_context",
                    "Set context for a user's nickname.",
                    PermissionType::ServerPerms(Permissions::MANAGE_NICKNAMES),
                    Some(Box::new(move |ctx, command, params| {
                        Box::pin(async move {
                            let user = get_param!(params, User, "user");
                            let user = command.data.resolved.users.get(user).unwrap();
                            let n = *get_param!(params, Integer, "number");
                            let guild_id = command.guild_id.unwrap();

                            if n < 1 {
                                return Ok(Some(ActionResponse::new(
                                        create_raw_embed("**`number` must be greater than 0**
Check the user's nickname list for valid numbers to remove!"),
                                        true,
                                    )))
                            }

                            info!(
                                "[Guild: {}] Updating context for nickname #{n} for {} ({})",
                                guild_id, user.name, user.id,
                            );

                            let data = crate::acquire_data_handle!(read ctx);
                            let guild = get_guild(&data, &guild_id).unwrap();
                            let nickname_lottery_data = guild.nickname_lottery_data();

                            if nickname_lottery_data.user_nicknames(&user.id).map(|nicknames| n as usize > nicknames.len()).unwrap_or(true) {
                                info!(
                                    "[Guild: {}] Nickname #{n} does not exist for {} ({}); ignoring.",
                                    guild_id, user.name, user.id
                                );
                                return Ok(Some(ActionResponse::new(
                                    create_raw_embed(format!("**Nickname #{n} does not exist for {}**
Consider checking their nickname list for valid number to remove.",
                                        user.mention())),
                                    true,
                                )));
                            }
                            let nickname = &nickname_lottery_data.user_nicknames(&user.id).unwrap()[n as usize - 1].clone();
                            crate::drop_data_handle!(data);

                            let input_context = serenity::builder::CreateInputText::new(
                                serenity::all::InputTextStyle::Paragraph,
                                "Nickname context",
                                "nickname_context",
                            )
                            .placeholder("Any context about this nickname to offset future forgetfulness.")
                            .value(nickname.context().cloned().unwrap_or_else(|| String::from("")))
                            .required(true);

                            let components =
                                vec![serenity::all::CreateActionRow::InputText(input_context)];

                            command
                                .create_response(
                                    &ctx.http(),
                                    serenity::all::CreateInteractionResponse::Modal(
                                        CreateModal::new(
                                            user.id.to_string() + "_" + nickname.nickname() + "_context",
                                            format!("Context for {}", nickname.nickname()),
                                        )
                                        .components(components),
                                    ),
                                )
                                .await?;

                            let userid = user.id;
                            let nick = nickname.nickname().clone();
                            // collect the submitted data
                            if let Some(int) = serenity::collector::ModalInteractionCollector::new(ctx)
                                .filter(move |int| {
                                    int.data.custom_id == userid.to_string() + "_" + &nick + "_context"
                                })
                                .timeout(Duration::new(300, 0))
                                .await
                            {
                                let mut data = crate::acquire_data_handle!(write ctx);
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
                                    if let serenity::all::ActionRowComponent::InputText(it) = input {
                                        if it.custom_id == "nickname_context" {
                                            if let Some(it) = &it.value {
                                                if !it.is_empty() {
                                                    nickname_lottery_data.set_user_nickname_context(&user.id, n as usize, it.to_string());
                                                }
                                            }
                                        }
                                    }
                                }

                                config.save();

                                crate::drop_data_handle!(data);

                                // it's now safe to close the modal, so send a response to it
                                int.create_response(
                                    &ctx.http(),
                                    serenity::all::CreateInteractionResponse::Acknowledge,
                                )
                                .await?;
                            }
                            Ok(None)
                        })
                    })),
                )
                .add_option(crate::Option::new(
                    "user",
                    "The user to add the nickname for.",
                    OptionType::User,
                    true,
                ))
                .add_option(crate::Option::new(
                    "number",
                    "The number of the nickname to remove, as reported in the user's nickname list.",
                    OptionType::IntegerInput(Some(1), None),
                    true,
                )),
            )
            .add_variant(
                Command::new(
                    "info",
                    "Get more information about a nickname for a user.",
                    PermissionType::ServerPerms(Permissions::USE_APPLICATION_COMMANDS),
                    Some(Box::new(move |ctx, command, params| {
                        Box::pin(async move {
                            let user = get_param!(params, User, "user");
                            let user = command.data.resolved.users.get(user).unwrap();
                            let n = *get_param!(params, Integer, "number");
                            let guild_id = command.guild_id.unwrap();

                            if n < 1 {
                                return Ok(Some(ActionResponse::new(
                                        create_raw_embed("**`number` must be greater than 0**
Check the user's nickname list for valid numbers!"),
                                        true,
                                    )))
                            }

                            let data = crate::acquire_data_handle!(read ctx);
                            let guild = get_guild(&data, &guild_id).unwrap();
                            let nickname_lottery_data = guild.nickname_lottery_data();

                            if nickname_lottery_data.user_nicknames(&user.id).map(|nicknames| n as usize > nicknames.len()).unwrap_or(true) {
                                info!(
                                    "[Guild: {}] Nickname #{n} does not exist for {} ({}); ignoring.",
                                    guild_id, user.name, user.id
                                );
                                return Ok(Some(ActionResponse::new(
                                    create_raw_embed(format!("**Nickname #{n} does not exist for {}**
Consider checking their nickname list for valid number to remove.",
                                        user.mention())),
                                    true,
                                )));
                            }
                            let nickname = &nickname_lottery_data.user_nicknames(&user.id).unwrap()[n as usize - 1].clone();
                            crate::drop_data_handle!(data);

                            Ok(Some(ActionResponse::new(
                                create_raw_embed(
                                    format!("**Nickname '{}' for {}**
Originally added by {} ({})
**Context:**
{}",
                                    nickname.nickname(), user.mention(),
                                    nickname.author()
                                            .map(|uid| uid.mention().to_string())
                                            .unwrap_or("`user not known`".to_string()),
                                    nickname.time()
                                            .map(|time| format!("<t:{}:F>", time.timestamp().to_string()))
                                            .unwrap_or("`time not known`".to_string()),
                                    nickname.context()
                                            .unwrap_or(&"No context provided.".to_string()),
                                    )
                                ),
                                true,
                            )))
                        })
                    })),
                )
                .add_option(crate::Option::new(
                    "user",
                    "The user whose nickname you seek more information about.",
                    OptionType::User,
                    true,
                ))
                .add_option(crate::Option::new(
                    "number",
                    "The number of the nickname to get information about, as reported in the user's nickname list.",
                    OptionType::IntegerInput(Some(1), None),
                    true,
                )),
            )
            .add_variant(
                Command::new(
                    "list",
                    "List all nicknames set for the user.",
                    PermissionType::ServerPerms(Permissions::USE_APPLICATION_COMMANDS),
                    Some(Box::new(move |ctx, command, params| {
                        Box::pin(async {
                            let user = get_param!(params, User, "user");
                            let user = command.data.resolved.users.get(user).unwrap();
                            let data = crate::acquire_data_handle!(read ctx);
                            if let Some(guild) = get_guild(&data, &command.guild_id.unwrap()) {
                                let lottery_data = guild.nickname_lottery_data();
                                if let Some(nicknames) = lottery_data.user_nicknames(&user.id) {
                                    let mut list = format!("**Nicknames for {}**", user.mention());
                                    for (i, nickname) in nicknames.iter().enumerate() {
                                        list += &format!("\n{}. {}", i + 1, nickname.nickname());
                                    }
                                    Ok(Some(ActionResponse::new(
                                        create_raw_embed(list),
                                        true,
                                    )))
                                } else {
                                    Ok(Some(ActionResponse::new(
                                        create_raw_embed(format!("{} has no nicknames in this server.", user.mention())),
                                        true,
                                    )))
                                }
                            } else {
                                error!("Guild command called in an unitialised guild {}", command.guild_id.unwrap());
                                Ok(None)
                            }
                        })
                    })),
                )
                .add_option(crate::Option::new(
                    "user",
                    "The user to get the nickname list for.",
                    OptionType::User,
                    true,
                ))
            )
        )
        .add_variant(
            Command::new(
                "refresh_interval",
                "The frequency at which the nickname lottery can occur, changing a single random user's nickname.",
                PermissionType::ServerPerms(Permissions::MANAGE_NICKNAMES),
                None,
            )
            .add_variant(
                Command::new(
                    "set",
                    "Set a custom interval range for this server. Does not affect April Fool's day.",
                    PermissionType::ServerPerms(Permissions::MANAGE_NICKNAMES),
                    Some(Box::new(move |ctx, command, params| {
                        Box::pin(async {
                            let min = get_param!(params, Integer, "min");
                            let max = get_param!(params, Integer, "max");

                            let mut data = crate::acquire_data_handle!(write ctx);
                            let config = data.get_mut::<Config>().unwrap();
                            let guild = config.guild_mut(&command.guild_id.unwrap());
                            let nickname_lottery_data = guild.nickname_lottery_data_mut();
                            nickname_lottery_data.set_refresh_interval(Some((*min as u64, *max as u64)));
                            config.save();
                            crate::drop_data_handle!(data);

                            let format_time = |secs| -> String {
                                let seconds = secs % 60;
                                let minutes = (secs / 60) % 60;
                                let hours = (secs / 60 / 60) % 24;
                                let days = secs / 60 / 60 / 24;

                                format!("{days}d {hours}h {minutes}m {seconds}s")
                            };

                            let resp = format!(
                                "**Nickname lottery refresh interval updated**
Minimum time between lotteries: {}
Maximum time between lotteries: {}",
                                format_time(min), format_time(max)
                            );
                            Ok(Some(ActionResponse::new(create_raw_embed(resp), true)))
                        })
                    })),
                )
                .add_option(crate::command::Option::new(
                    "min",
                    "The minimum time, in seconds, between nickname changes.",
                    OptionType::IntegerInput(Some(1_800), None),
                    true,
                ))
                .add_option(crate::command::Option::new(
                    "max",
                    "The maximum time, in seconds, between nickname changes.",
                    OptionType::IntegerInput(Some(1_800), None),
                    true,
                )),
            )
            .add_variant(
                Command::new(
                    "reset",
                    "Revert back to the default interval.",
                    PermissionType::ServerPerms(Permissions::MANAGE_NICKNAMES),
                    Some(Box::new(move |ctx, command, _params| {
                        Box::pin(async {
                            let mut data = crate::acquire_data_handle!(write ctx);
                            let config = data.get_mut::<Config>().unwrap();
                            let guild = config.guild_mut(&command.guild_id.unwrap());
                            let lottery_data = guild.nickname_lottery_data_mut();
                            lottery_data.set_refresh_interval(None);
                            config.save();
                            crate::drop_data_handle!(data);

                            Ok(Some(ActionResponse::new(
                                create_raw_embed("Refresh interval has been reset to default."),
                                true,
                            )))
                        })
                    })),
                )
            )
        )
        .add_variant(
            Command::new(
                "announcements",
                "Commands to manage nickname lottery announcements.",
                PermissionType::ServerPerms(Permissions::MANAGE_CHANNELS),
                None,
            )
            .add_variant(
                Command::new(
                    "configure",
                    "Configure announcements when the bot fails to change a user's nickname.",
                    PermissionType::ServerPerms(Permissions::MANAGE_CHANNELS),
                    Some(Box::new(move |ctx, command, params| {
                        Box::pin(async {
                            // Set announcement channel if it's been supplied.
                            if let Some(channel_opt) =
                                params.iter().find(|opt| opt.name == "channel")
                            {
                                let mut data = crate::acquire_data_handle!(write ctx);
                                let config = data.get_mut::<Config>().unwrap();
                                let guild = config.guild_mut(&command.guild_id.unwrap());
                                if let CommandDataOptionValue::Channel(channel) = &channel_opt.value
                                {
                                    let channel = channel.to_channel(&ctx.http()).await?;
                                    guild
                                        .nickname_lottery_data_mut()
                                        .set_channel(Some(channel.id()));
                                    config.save();
                                }
                            };

                            // Set title override if it's been supplied.
                            if let Some(title_opt) =
                                params.iter().find(|opt| opt.name == "title_override")
                            {
                                let mut data = crate::acquire_data_handle!(write ctx);
                                let config = data.get_mut::<Config>().unwrap();
                                let guild = config.guild_mut(&command.guild_id.unwrap());
                                let lottery_data = guild.nickname_lottery_data_mut();
                                if let CommandDataOptionValue::String(title_override) =
                                    &title_opt.value
                                {
                                    lottery_data
                                        .set_title_override(Some(title_override.to_owned()));
                                    config.save();
                                }
                            };

                            let data = crate::acquire_data_handle!(read ctx);
                            let guild = get_guild(&data, &command.guild_id.unwrap());
                            let lottery_data = &guild.unwrap().nickname_lottery_data();
                            let resp = format!(
                                "**Nickname lottery complaints channel updated!**
Channel: {}
Title text: {}",
                                lottery_data
                                    .channel()
                                    .unwrap()
                                    .to_channel(&ctx.http())
                                    .await?,
                                lottery_data.title()
                            );
                            Ok(Some(ActionResponse::new(create_raw_embed(resp), true)))
                        })
                    })),
                )
                .add_option(crate::command::Option::new(
                    "channel",
                    "The channel to announce timeouts in.",
                    OptionType::Channel(Some(vec![ChannelType::Text])),
                    false,
                ))
                .add_option(crate::command::Option::new(
                    "title_override",
                    "Text to prepend before the timeout counter message.",
                    OptionType::StringInput(None, None),
                    false,
                )),
            )
            .add_variant(Command::new(
                "stop",
                "Stop all announcements. Unsets all configuration values.",
                PermissionType::ServerPerms(Permissions::MANAGE_CHANNELS),
                Some(Box::new(move |ctx, command, _params| {
                    Box::pin(async {
                        let mut data = crate::acquire_data_handle!(write ctx);
                        let config = data.get_mut::<Config>().unwrap();
                        let guild = config.guild_mut(&command.guild_id.unwrap());
                        let lottery_data = guild.nickname_lottery_data_mut();
                        lottery_data.set_channel(None);
                        lottery_data.set_title_override(None);
                        config.save();
                        crate::drop_data_handle!(data);

                        Ok(Some(ActionResponse::new(
                            create_raw_embed("Announcements have been uninitialised."),
                            true,
                        )))
                    })
                })),
            )),
        )]
    }
}

impl NicknameLottery {
    pub async fn guild_init(ctx: Context, g: Guild) {
        // between 30 minutes and 5 days
        let mut interval = DEFAULT_REFRESH_INTERVAL;
        let mut between = rand::distributions::Uniform::from(
            DEFAULT_REFRESH_INTERVAL.0..DEFAULT_REFRESH_INTERVAL.1,
        );
        loop {
            // Use a different distribution if the guild's set a different refresh interval.
            let data = crate::acquire_data_handle!(read ctx);
            if let Some(guild) = get_guild(&data, &g.id) {
                let lottery_data = guild.nickname_lottery_data();
                let i = if let Some(i) = lottery_data.refresh_interval() {
                    *i
                } else {
                    DEFAULT_REFRESH_INTERVAL
                };
                if interval.0 != i.0 || interval.1 != i.1 {
                    interval = i;
                    between = rand::distributions::Uniform::from(interval.0..interval.1);
                }
            }
            crate::drop_data_handle!(data);
            let now = chrono::Utc::now();
            let is_april_fools = now.month() == 4 && now.day() == 1;
            if cfg!(not(debug_assertions)) {
                let mut tts = Duration::from_secs(between.sample(&mut rand::thread_rng()));
                // It's April Fool's! Force the minimum refresh interval.
                if is_april_fools {
                    tts = Duration::from_secs(1_800);
                } else if now.month() < 4 {
                    let ctts = chrono::Duration::from_std(tts);
                    match ctts {
                        Ok(ctts) => {
                            if (now + ctts).month() >= 4 {
                                // Current reset timer will either cross into, or completely skip, April Fool's.
                                // Clamp to time until April Fool's.
                                tts = match chrono::Utc
                                    .with_ymd_and_hms(now.year(), 4, 1, 0, 0, 0)
                                    .unwrap()
                                    .signed_duration_since(now)
                                    .to_std()
                                {
                                    Ok(tts) => tts,
                                    Err(e) => {
                                        #[cfg(feature = "events")]
                                        notify_subscribers(
                                            &ctx,
                                            Event::Error,
                                            &format!(
                                                "**[Guild: {}] Error calculating time until next nickname change:**
{e}

_Nickname changes are disabled for this guild until next initialisation._",
                                                g.id
                                            ),
                                        )
                                        .await;
                                        panic!("OutOfRangeError during reset time calculation.");
                                    }
                                };
                            }
                        }
                        Err(e) => {
                            #[cfg(feature = "events")]
                            notify_subscribers(
                                &ctx,
                                Event::Error,
                                &format!(
                                    "**[Guild: {}] Error calculating time until next nickname change:**
{e}

_Nickname changes are disabled for this guild until next initialisation._",
                                    g.id
                                ),
                            )
                            .await;
                            panic!("OutOfRangeError during reset time calculation.");
                        }
                    }
                }
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
            let data = crate::acquire_data_handle!(read ctx);
            if let Some(guild) = get_guild(&data, &g.id) {
                let lottery_data = guild.nickname_lottery_data();
                if let Some(user) = lottery_data.get_random_user() {
                    if let Ok(member) = g.member(&ctx.http(), user).await {
                        let user = &member.user;
                        if let Some(mut new_nick) =
                            lottery_data.get_nickname_for_user(&user.id).cloned()
                        {
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
                            if old_nick == new_nick {
                                info!("[Guild: {}] Skipping nickname change for {} ({}) as they pulled the same as current: {}.", &g.id, &user.id, &old_nick, &new_nick);
                                continue;
                            }
                            info!(
                                "[Guild: {}] Updating {}'s nickname to {} (current: {})",
                                &g.id, &user.id, &new_nick, &old_nick
                            );
                            let mut post_name_change = is_april_fools;
                            if let Err(e) = g
                                .edit_member(
                                    &ctx.http(),
                                    user.id,
                                    serenity::all::EditMember::new().nickname(&new_nick),
                                )
                                .await
                            {
                                post_name_change = true;
                                warn!(
                                    "[Guild: {}] Error changing {}'s nickname:
{e}",
                                    g.id, user.id
                                );
                            }
                            if post_name_change {
                                if let Some(channel_id) = lottery_data.channel() {
                                    let channel = match channel_id.to_channel(&ctx.http()).await {
                                        Ok(channel) => channel.guild(),
                                        Err(_) => None,
                                    };
                                    if let Some(channel) = channel {
                                        channel
                                            .send_message(
                                                &ctx.http(),
                                                create_embed(format!(
                                                    "**{}**
{} won/lost the lottery! From now on, they are to be named: `{}`",
                                                    lottery_data.title(),
                                                    user.mention(),
                                                    new_nick,
                                                )),
                                            )
                                            .await
                                            .unwrap();
                                    } else {
                                        #[cfg(feature = "events")]
                                        notify_subscribers_with_handle(
                                            &ctx,
                                            &data,
                                            Event::Error,
                                            &format!(
                                                "**[Guild: {}] Invalid complaints channel.**",
                                                g.id,
                                            ),
                                        )
                                        .await;
                                        error!("[Guild: {}] Invalid complaints channel.", g.id);
                                        continue;
                                    }
                                }
                            }
                        }
                    }
                }
            }
            // Only run once in debug mode.
            if cfg!(debug_assertions) {
                break;
            }
        }
    }
}

#[cfg(test)]
mod test {
    use serenity::model::prelude::UserId;

    use super::{NicknameData, NicknameLotteryGuildData};

    #[test]
    fn test_setting_and_selecting_nicknames() {
        let users = [UserId::from(1), UserId::from(2)];
        let mut data: NicknameLotteryGuildData = NicknameLotteryGuildData::default();
        assert_eq!(data.get_nickname_for_user(&users[0]), None);
        assert_eq!(data.get_nickname_for_user(&users[1]), None);
        data.add_user_nickname(
            &users[0],
            NicknameData {
                nickname: String::from("user0"),
                author: None,
                time: None,
                context: None,
            },
        );
        data.add_user_nickname(
            &users[1],
            NicknameData {
                nickname: String::from("user1"),
                author: None,
                time: None,
                context: None,
            },
        );
        assert_eq!(
            data.get_nickname_for_user(&users[0]),
            Some(&"user0".to_string())
        );
        assert_eq!(
            data.get_nickname_for_user(&users[1]),
            Some(&"user1".to_string())
        );
        data.remove_user_nickname(&users[0], 1);
        assert_eq!(data.get_nickname_for_user(&users[0]), None);
        assert_eq!(
            data.get_nickname_for_user(&users[1]),
            Some(&"user1".to_string())
        );
    }

    #[test]
    fn select_random_user() {
        let users = [UserId::from(1)];
        let mut data: NicknameLotteryGuildData = NicknameLotteryGuildData::default();
        assert_eq!(data.get_random_user(), None);
        data.add_user_nickname(
            &users[0],
            NicknameData {
                nickname: String::from("user0"),
                author: None,
                time: None,
                context: None,
            },
        );
        assert_eq!(data.get_random_user(), Some(users[0].clone()));
        data.remove_user_nickname(&users[0], 1);
        assert_eq!(data.get_random_user(), None);
    }
}
