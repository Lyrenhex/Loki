use log::error;
use serenity::{
    all::{CacheHttp as _, EditMember},
    async_trait,
    model::prelude::{ActivityType, GuildId, Presence},
    prelude::Context,
};

use crate::{command::notify_subscribers, config::Config};

use super::Subsystem;

pub const STREAMING_PREFIX: &str = "🔴 ";

pub struct StreamIndicator;

#[async_trait]
impl Subsystem for StreamIndicator {
    fn generate_commands(&self) -> Vec<crate::command::Command<'static>> {
        vec![]
    }

    async fn presence(&self, ctx: &Context, new_data: &Presence) {
        let data = crate::acquire_data_handle!(read ctx);
        let config = data.get::<Config>().unwrap();
        if let Some(activity) = new_data
            .activities
            .iter()
            .find(|a| a.kind == ActivityType::Streaming)
        {
            if let Some(user) = new_data.user.to_user() {
                let mut notify = true;
                for guild in config
                    .guilds()
                    .map(|g| GuildId::new(g.parse::<u64>().unwrap()))
                {
                    let nick = user
                        .nick_in(&ctx.http(), guild)
                        .await
                        .unwrap_or(user.name.clone());
                    if !nick.starts_with(STREAMING_PREFIX) {
                        let old_nick = nick.clone();
                        let nick = STREAMING_PREFIX.to_owned()
                            + &nick.chars().take(30).collect::<String>();
                        if let Ok(guild) = guild.to_partial_guild(&ctx.http()).await {
                            if let Err(e) = guild
                                .edit_member(
                                    &ctx.http(),
                                    user.id,
                                    EditMember::new().nickname(&nick),
                                )
                                .await
                            {
                                error!("Nickname update failed: {old_nick} -> {nick}\n{:?}", e);
                            }
                        }
                    } else {
                        // we've already set the prefix - don't spam users, in
                        // case another we don't have permission to set the
                        // prefix in another server!
                        notify = false;
                    }
                }
                crate::drop_data_handle!(data);
                if notify {
                    notify_subscribers(
                        ctx,
                        super::events::Event::Stream,
                        format!(
                            "**{} is now live!**",
                            if let Some(url) = &activity.url {
                                format!("[{}]({})", &user.name, url)
                            } else {
                                user.name
                            },
                        )
                        .as_str(),
                    )
                    .await;
                }
            }
        } else if let Some(user) = new_data.user.to_user() {
            for guild in config
                .guilds()
                .map(|g| GuildId::new(g.parse::<u64>().unwrap()))
            {
                let nick = user.nick_in(&ctx.http(), guild).await;
                if let Some(nick) = nick {
                    if nick.starts_with(STREAMING_PREFIX) {
                        // the user isn't streaming any more, but they are still marked as such.
                        let old_nick = nick.clone();
                        let nick = nick.chars().skip(2).collect::<String>();
                        if let Ok(guild) = guild.to_partial_guild(&ctx.http()).await {
                            if let Err(e) = guild
                                .edit_member(
                                    &ctx.http(),
                                    user.id,
                                    EditMember::new().nickname(&nick),
                                )
                                .await
                            {
                                error!("Nickname update failed: {old_nick} -> {nick}\n{:?}", e);
                            }
                        }
                    }
                }
            }
        }
    }
}
