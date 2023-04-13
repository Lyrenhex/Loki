use chrono::{DateTime, Utc};
use log::info;
use serde::{Deserialize, Serialize};
use serenity::{
    async_trait,
    model::{
        prelude::{interaction::application_command::CommandDataOptionValue, Member},
        Permissions, Timestamp,
    },
    prelude::{Context, Mentionable},
};

use crate::{
    command::{create_response, Command, OptionType, PermissionType},
    config::{get_guild, Config},
};

use super::Subsystem;

#[derive(Serialize, Deserialize, Default)]
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
            PermissionType::ServerPerms(Permissions::USE_SLASH_COMMANDS),
            Some(Box::new(move |ctx, command| {
                Box::pin(async {
                    let user = if let Some(CommandDataOptionValue::User(user, _)) =
                        &command.data.options[0].resolved
                    {
                        user
                    } else {
                        return Err(crate::Error::InvalidUser);
                    };
                    let data = ctx.data.read().await;
                    let mut resp = format!("{} hasn't been timed out!", user.mention());
                    if let Some(guild) = get_guild(&data, &command.guild_id.unwrap()) {
                        if let Some(timeouts) = guild.timeouts() {
                            if let Some(utd) = timeouts.get(&user.id.to_string()) {
                                resp = format!("{} has been timed out **{}** time(s), for a total of **{} second(s)**.", user.mention(), utd.count, utd.total_time);
                            }
                        }
                    }
                    create_response(&ctx.http, command, &resp, false).await;
                    Ok(())
                })
            })),
        )
        .add_option(crate::command::Option::new(
            "user",
            "The user to view timeout statistics of.",
            OptionType::User,
            true,
        ))]
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
        let mut data = ctx.data.write().await;
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
                    config.save();
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
