use log::error;
use serenity::{
    async_trait,
    model::prelude::{ActivityType, GuildId, Presence},
    prelude::Context,
};

use crate::{command::notify_subscribers, config::Config};

use super::Subsystem;

const STREAMING_PREFIX: &str = "🔴 ";

pub struct StreamIndicator;

#[async_trait]
impl Subsystem for StreamIndicator {
    fn generate_commands(&self) -> Vec<crate::command::Command<'static>> {
        vec![]
    }

    async fn presence(&self, ctx: &Context, new_data: &Presence) {
        let data = ctx.data.read().await;
        let config = data.get::<Config>().unwrap();
        if let Some(activity) = new_data
            .activities
            .iter()
            .find(|a| a.kind == ActivityType::Streaming)
        {
            if let Some(user) = new_data.user.to_user() {
                for guild in config.guilds().map(|g| GuildId(g.parse::<u64>().unwrap())) {
                    let nick = user
                        .nick_in(&ctx.http, guild)
                        .await
                        .unwrap_or(user.name.clone());
                    if !nick.starts_with(STREAMING_PREFIX) {
                        // the user is streaming, but they aren't marked as such.
                        // first, notify subscribers that someone's live!
                        notify_subscribers(
                            ctx,
                            super::events::Event::Stream,
                            format!(
                                "**{} is now live!**",
                                if let Some(url) = &activity.url {
                                    format!("[{}]({})", &nick, url)
                                } else {
                                    nick.clone()
                                },
                            )
                            .as_str(),
                        )
                        .await;
                        let old_nick = nick.clone();
                        let nick = STREAMING_PREFIX.to_owned()
                            + &nick.chars().take(30).collect::<String>();
                        if let Ok(guild) = guild.to_partial_guild(&ctx.http).await {
                            if let Err(e) = guild
                                .edit_member(&ctx.http, user.id, |u| u.nickname(nick.clone()))
                                .await
                            {
                                error!("Nickname update failed: {old_nick} -> {nick}\n{:?}", e);
                            }
                        }
                    }
                }
            }
        } else if let Some(user) = new_data.user.to_user() {
            for guild in config.guilds().map(|g| GuildId(g.parse::<u64>().unwrap())) {
                let nick = user.nick_in(&ctx.http, guild).await;
                if let Some(nick) = nick {
                    if nick.starts_with(STREAMING_PREFIX) {
                        // the user isn't streaming any more, but they are still marked as such.
                        let old_nick = nick.clone();
                        let nick = nick.chars().skip(2).collect::<String>();
                        if let Ok(guild) = guild.to_partial_guild(&ctx.http).await {
                            if let Err(e) = guild
                                .edit_member(&ctx.http, user.id, |u| u.nickname(nick.clone()))
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
