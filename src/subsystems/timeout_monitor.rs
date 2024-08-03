use chrono::{DateTime, Utc};
use log::{error, info};
use serde::{Deserialize, Serialize};
use serenity::{
    all::Mentionable as _,
    async_trait, futures,
    model::{
        application::CommandDataOptionValue,
        id::UserId,
        prelude::{Channel, ChannelId, ChannelType, Member},
        Permissions, Timestamp,
    },
    prelude::Context,
};
use tinyvec::array_vec;

use crate::{
    command::{Command, OptionType, PermissionType},
    config::{get_guild, Config},
    create_embed, create_raw_embed, ActionResponse,
};

use super::Subsystem;

const ANNOUNCEMENT_TEXT: &str = "[User] has been timed out [x] times now!";

/// Configuration for the announcements in a specific guild.
#[derive(Serialize, Deserialize)]
pub struct AnnouncementsConfig {
    /// Channel to announce in.
    channel: ChannelId,
    /// Prefix to prepend before the number of times a user was timed out, during an announcement.
    prefix: String,
}

impl AnnouncementsConfig {
    /// Construct a new [AnnouncementsConfig] with the given announcements channel.
    pub fn new(channel: Channel) -> Self {
        Self {
            channel: channel.id(),
            prefix: String::default(),
        }
    }

    /// ID of the channel to announce in.
    pub fn channel(&self) -> ChannelId {
        self.channel
    }

    /// Set the channel to announce in.
    pub fn set_channel(&mut self, channel: Channel) {
        self.channel = channel.id();
    }

    /// Prefix to prepend before the number of times a user was timed out, during an announcement.
    pub fn prefix(&self) -> &String {
        &self.prefix
    }

    /// Set the prefix to append before the number of times a user was timed out in the announcement.
    pub fn set_prefix(&mut self, prefix: &str) {
        self.prefix = prefix.into();
    }

    pub fn announcement_text(&self) -> String {
        format!(
            "{}{}{}",
            self.prefix(),
            if self.prefix() != "" { " " } else { "" },
            ANNOUNCEMENT_TEXT
        )
    }
}

#[derive(Serialize, Deserialize, Default, Clone, Copy)]
pub struct UserTimeoutData {
    /// Total number of timeouts that have been noticed.
    count: i64,
    /// Total number of seconds that the user has been timed out for.
    total_time: i64,
    /// The timestamp of the last time Loki witnessed the user being timed out.
    last_timed_out: Option<DateTime<Utc>>,
    /// The timestamp that the current timeout is expected to end.
    expected_expiry: Option<Timestamp>,
}

pub struct TimeoutMonitor;

#[async_trait]
impl Subsystem for TimeoutMonitor {
    fn generate_commands(&self) -> Vec<crate::command::Command<'static>> {
        vec![Command::new(
            "timeouts",
            "Timeout statistics for a given user.",
            PermissionType::ServerPerms(Permissions::USE_APPLICATION_COMMANDS),
            None,
        )
        .add_variant(Command::new(
            "check",
            "Check timeout statistics for a given user.",
            PermissionType::ServerPerms(Permissions::USE_APPLICATION_COMMANDS),
            Some(Box::new(move |ctx, command, params| {
                Box::pin(async {
                    let user = get_param!(params, User, "user");
                    let data = crate::acquire_data_handle!(read ctx);
                    let mut resp = format!("{} hasn't been timed out!", user.mention());
                    if let Some(guild) = get_guild(&data, &command.guild_id.unwrap()) {
                        if let Some(timeouts) = guild.timeouts() {
                            if let Some(utd) = timeouts.get(&user.to_string()) {
                                resp = format!("{} has been timed out **{}** time(s), for a total of **{} second(s)**.", user.mention(), utd.count, utd.total_time);
                            }
                        }
                    }
                    Ok(Some(ActionResponse::new(create_raw_embed(resp), false)))
                })
            })),
        )
        .add_option(crate::command::Option::new(
            "user",
            "The user to view timeout statistics of.",
            OptionType::User,
            true,
        )))
        .add_variant(Command::new(
            "configure_announcements",
            "Configure announcements when a user is timed out.",
            PermissionType::ServerPerms(Permissions::MANAGE_CHANNELS),
            Some(Box::new(move |ctx, command, params| {
                Box::pin(async move {
                    // Set announcement channel if it's been supplied.
                    if let Some(channel_opt) = params.iter().find(|opt| opt.name == "channel") {
                        let mut data = crate::acquire_data_handle!(write ctx);
                        let config = data.get_mut::<Config>().unwrap();
                        let guild = config.guild_mut(&command.guild_id.unwrap());
                        if let CommandDataOptionValue::Channel(channel) = &channel_opt.value {
                            let channel = channel.to_channel(&ctx).await?;
                            if let Some(announcement_config) = guild.timeouts_announcement_config_mut() {
                                announcement_config.set_channel(channel);
                            } else {
                                guild.timeouts_announcement_init(channel);
                            }
                            config.save();
                        }
                    } else {
                        // No channel set - is there one already...?
                        // If not, for any reason, we should stop processing immediately.
                        let data = crate::acquire_data_handle!(read ctx);
                        let guild = get_guild(&data, &command.guild_id.unwrap());
                        // We don't know this guild.
                        if guild.is_none() {
                            return Ok(Some(ActionResponse::new(create_raw_embed("You must set an announcements channel first!"), true)));
                        }
                        let announcements_config = guild.unwrap().timeouts_announcement_config();
                        // No announcements channel set!
                        if announcements_config.is_none() {
                            return Ok(Some(ActionResponse::new(create_raw_embed("You must set an announcements channel first!"), true)));
                        }
                        // There is an announcements channel set, so we can continue with that.
                    };

                    // Set announcement prefix if it's been supplied.
                    if let Some(prefix_opt) = params.iter().find(|opt| opt.name == "announcement_prefix") {
                        let mut data = crate::acquire_data_handle!(write ctx);
                        let config = data.get_mut::<Config>().unwrap();
                        let guild = config.guild_mut(&command.guild_id.unwrap());
                        let announcement_config = guild.timeouts_announcement_config_mut().unwrap();
                        if let CommandDataOptionValue::String(prefix) = &prefix_opt.value {
                            announcement_config.set_prefix(prefix);
                            config.save();
                        }
                    };

                    let data = crate::acquire_data_handle!(read ctx);
                    let guild = get_guild(&data, &command.guild_id.unwrap());
                    let announcements_config = &guild.unwrap().timeouts_announcement_config().unwrap();
                    let resp = format!("**Timeouts announcement config updated!**
Channel: {}
Announcement text: {}",
                        announcements_config.channel().to_channel(&ctx).await?,
                        announcements_config.announcement_text());
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
            "announcement_prefix",
            "Text to prepend before the timeout counter message.",
            OptionType::StringInput(None, None),
            false,
        )))
        .add_variant(Command::new(
            "stop_announcements",
            "Stop all announcements. Unsets all configuration values.",
            PermissionType::ServerPerms(Permissions::MANAGE_CHANNELS),
            Some(Box::new(move |ctx, command, _params| {
                Box::pin(async {
                    let mut data = crate::acquire_data_handle!(write ctx);
                    let config = data.get_mut::<Config>().unwrap();
                    let guild = config.guild_mut(&command.guild_id.unwrap());
                    let announcements_config = guild.timeouts_announcement_config_mut();
                    // No announcements channel set!
                    if announcements_config.is_none() {
                        return Ok(Some(ActionResponse::new(create_raw_embed("Announcements haven't been set up yet."), true)));
                    }
                    // There is an announcements channel set.
                    guild.timeouts_announcement_uninit();
                    config.save();
                    crate::drop_data_handle!(data);

                    Ok(Some(ActionResponse::new(create_raw_embed("Announcements have been uninitialised."), true)))
                })
            })),
        ))
        .add_variant(Command::new(
            "leaderboard",
            "Display the leaderboard for timeout statistics.",
            PermissionType::ServerPerms(Permissions::USE_APPLICATION_COMMANDS),
            Some(Box::new(move |ctx, command, params| {
                Box::pin(async move {
                    let metric = get_param!(params, String, "metric").to_lowercase();
                    let mut users = String::new();
                    let mut counts = String::new();
                    let mut times = String::new();
                    let sort_by = |(_, utd_a): &(String, UserTimeoutData), (_uid_b, utd_b): &(String, UserTimeoutData)| {
                        match metric.as_str() {
                            "quantity" => utd_b.count.cmp(&utd_a.count),
                            "total time" => utd_b.total_time.cmp(&utd_a.total_time),
                            _ => unreachable!() }
                    };
                    let data = crate::acquire_data_handle!(read ctx);
                    if let Some(guild) = get_guild(&data, &command.guild_id.unwrap()) {
                        if let Some(timeouts) = guild.timeouts() {
                            let mut entries = timeouts.iter().map(|(uid, utd)| (uid.clone(), *utd)).collect::<Vec<(String, UserTimeoutData)>>();
                            entries.sort_unstable_by(sort_by);
                            let iter = entries.iter().take(10);
                            users = futures::future::try_join_all(iter.clone().map(|(uid, _)| async {
                                Ok::<String, crate::Error>(UserId::from(uid.parse::<u64>().unwrap()).to_user(&ctx).await?.mention().to_string())
                            })).await?.join("\n");
                            counts = iter.clone().map(|(_, utd)| { utd.count.to_string() }).collect::<Vec<String>>().join("\n");
                            times = iter.map(|(_, utd)| {
                                let seconds = utd.total_time % 60;
                                let minutes = (utd.total_time / 60) % 60;
                                let hours = utd.total_time / 60 / 60;
                                format!("{hours}h {minutes}m {seconds}s")
                            }).collect::<Vec<String>>().join("\n");
                        }
                    }
                    let resp = create_raw_embed(format!("**Top 10 Timeout leaderboard** (sorted by {metric})")).field("User", users, true).field("Count", counts, true).field("Total time", times, true);
                    Ok(Some(ActionResponse::new(resp, false)))
                })
            })),
        )
        .add_option(crate::command::Option::new(
            "metric",
            "Metric to sort by.",
            OptionType::StringSelect(Box::new(array_vec!("Quantity".to_string(), "Total time".to_string()))),
            true,
        )))]
    }

    async fn member(&self, ctx: &Context, old: &Option<Member>, new: &Member) {
        let now = Utc::now();
        info!(
            "Handling potential timeout change for user {}: {:?}",
            new.user.id, new.communication_disabled_until
        );
        if let Some(old) = old {
            // return on boring cases (no change to timeout status and they've not just finished a timeout...)
            if new.communication_disabled_until == old.communication_disabled_until {
                if let Some(communication_disabled_until) = new.communication_disabled_until {
                    if communication_disabled_until > Utc::now().into() {
                        return;
                    }
                } else {
                    return;
                }
            }
        }
        let mut data = crate::acquire_data_handle!(write ctx);
        let config = data.get_mut::<Config>().unwrap();
        let guild = config.guild_mut(&new.guild_id);
        if let Some(communication_disabled_until) = new.communication_disabled_until {
            // User is currently timed out! We should check if this is new...
            if communication_disabled_until > now.into() {
                let mut is_new_timeout = false;
                if let Some(timeouts) = guild.timeouts() {
                    if let Some(utd) = timeouts.get(&new.user.id.to_string()) {
                        if let Some(expected_expiry) = utd.expected_expiry {
                            if communication_disabled_until > expected_expiry {
                                is_new_timeout = true;
                            }
                        } else {
                            is_new_timeout = true;
                        }
                    } else {
                        is_new_timeout = true;
                    }
                } else {
                    is_new_timeout = true;
                }
                if is_new_timeout {
                    // User is newly timed-out.
                    let utd = guild
                        .timeouts_mut()
                        .entry(new.user.id.to_string())
                        .or_default();
                    utd.last_timed_out = Some(now);
                    utd.expected_expiry = Some(communication_disabled_until);
                    utd.count += 1;
                    utd.total_time +=
                        (communication_disabled_until.with_timezone(&Utc) - now).num_seconds();
                    let count = utd.count;
                    config.save();
                    crate::drop_data_handle!(data);
                    let data = crate::acquire_data_handle!(read ctx);
                    let guild = get_guild(&data, &new.guild_id).unwrap();
                    if let Some(announcements_config) = guild.timeouts_announcement_config() {
                        if let Some(channel) = announcements_config
                            .channel
                            .to_channel(&ctx)
                            .await
                            .unwrap()
                            .guild()
                        {
                            channel
                                .send_message(
                                    &ctx,
                                    create_embed(format!(
                                        "{}{}{} has been timed out {} times now!",
                                        announcements_config.prefix(),
                                        if announcements_config.prefix() != "" {
                                            " "
                                        } else {
                                            ""
                                        },
                                        new.user.mention(),
                                        count,
                                    )),
                                )
                                .await
                                .unwrap();
                        } else {
                            error!(
                                "Invalid channel {} in guild {}",
                                announcements_config.channel, &new.guild_id
                            );
                        }
                    }
                }
            }
        } else {
            // User is not currently timed out! We should check if they *were*.
            if let Some(timeouts) = guild.timeouts() {
                if let Some(utd) = timeouts.get(&new.user.id.to_string()) {
                    if let Some(expected_expiry) = utd.expected_expiry {
                        if expected_expiry > now.into() {
                            // Interrupted timeout!
                            let utd = guild
                                .timeouts_mut()
                                .get_mut(&new.user.id.to_string())
                                .unwrap();
                            utd.total_time -=
                                (expected_expiry.with_timezone(&Utc) - now).num_seconds();
                            config.save();
                        }
                    }
                }
            }
        }
    }
}
