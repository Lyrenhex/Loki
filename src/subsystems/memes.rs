use std::time::Duration;

use chrono::{Days, Utc};
use log::{error, info};
use rand::Rng;
use serenity::{
    model::{
        prelude::{
            command::CommandOptionType, interaction::application_command::CommandDataOptionValue,
            Guild, Message,
        },
        Permissions,
    },
    prelude::{Context, Mentionable},
};

use crate::config::Config;
use crate::{
    command::{create_embed, create_response, Command, PermissionType},
    config::get_memes,
};

use super::Subsystem;

const REACTION_CHANCE: f64 = 0.1;
const REACTION_EMOTE: char = '🤖';

pub struct Memes;

impl Subsystem for Memes {
    fn generate_command() -> Command<'static> {
        Command::new(
            "memes",
            "Configuration commands for the meme-voting system.",
            PermissionType::ServerPerms(Permissions::MANAGE_CHANNELS),
            None,
        )
        .add_variant(
            Command::new(
                "set_channel",
                "Sets the memes channel for this server and initialises the meme subsystem.",
                PermissionType::ServerPerms(Permissions::MANAGE_CHANNELS),
                Some(Box::new(move |ctx, command| {
                    Box::pin(async {
                        let (channel_id, channel) =
                            if let Some(CommandDataOptionValue::Channel(channel)) =
                                &command.data.options[0].options[0].resolved
                            {
                                if let Some(channel) =
                                    channel.id.to_channel(&ctx.http).await?.guild()
                                {
                                    (channel.id, channel)
                                } else {
                                    return Err(crate::Error::InvalidChannel);
                                }
                            } else {
                                return Err(crate::Error::InvalidChannel);
                            };
                        let mut data = ctx.data.write().await;
                        let config = data.get_mut::<Config>().unwrap();
                        let guild_config = config.guild_mut(&command.guild_id.unwrap());
                        guild_config.set_memes_channel(Some(channel_id));
                        let reset_time = guild_config.memes().unwrap().next_reset();
                        config.save();
                        drop(data);
                        let resp = format!("Memes channel set to {}.", channel);
                        channel
                            .send_message(
                                &ctx.http,
                                create_embed(format!(
                                    "**Post your best memes!**
Vote by reacting to your favourite memes.
The post with the most total reactions by {} wins!",
                                    reset_time.format(crate::DATE_FMT),
                                )),
                            )
                            .await?;
                        create_response(&ctx.http, command, &resp).await;
                        Ok(())
                    })
                })),
            )
            .add_option(
                crate::command::Option::new(
                    "channel",
                    "The channel which is to be used for memes.",
                    CommandOptionType::Channel,
                    true,
                )
                .unwrap(),
            ),
        )
        .add_variant(Command::new(
            "unset_channel",
            "Unsets the memes channel for this server, resetting the meme subsystem.",
            PermissionType::ServerPerms(Permissions::MANAGE_CHANNELS),
            Some(Box::new(move |ctx, command| {
                Box::pin(async {
                    let mut data = ctx.data.write().await;
                    let config = data.get_mut::<Config>().unwrap();
                    let channel = config
                        .guild_mut(&command.guild_id.unwrap())
                        .memes()
                        .map(|memes| memes.channel());
                    config
                        .guild_mut(&command.guild_id.unwrap())
                        .set_memes_channel(None);
                    config.save();
                    drop(data);
                    let resp = "Memes channel unset.".to_string();
                    create_response(&ctx.http, command, &resp).await;
                    if let Some(channel) = channel {
                        if let Some(channel) = channel.to_channel(&ctx.http).await?.guild() {
                            channel
                                .send_message(
                                    &ctx.http,
                                    create_embed(
                                        "**Halt your memes!**
I won't see them anymore. :("
                                            .to_string(),
                                    ),
                                )
                                .await?;
                        }
                    }
                    Ok(())
                })
            })),
        ))
    }
}

impl Memes {
    /// Catch up on any messages that were missed while the bot was
    /// offline.
    pub async fn catch_up_messages(ctx: Context, g: &Guild) -> Context {
        let mut finished = true;
        let mut data = ctx.data.write().await;
        info!("Catching up with messages for guild {}...", g.id);
        let config = data.get_mut::<Config>().unwrap();
        let guild = config.guild_mut(&g.id);
        if let Some(memes) = guild.memes_mut() {
            // catch up on any messages that were missed while we were offline.
            if let Ok(channel) = memes.channel().to_channel(&ctx.http).await {
                let channel = channel.guild().unwrap();
                loop {
                    if let Some(last_message) = memes.list().last() {
                        match channel
                            .messages(&ctx.http, |retriever| {
                                retriever.after(last_message).limit(100)
                            })
                            .await
                        {
                            Ok(messages) => {
                                let messages: Vec<&Message> =
                                    messages.iter().filter(|m| !m.is_own(&ctx.cache)).collect();
                                messages.iter().for_each(|m| memes.add(m.id));
                                finished = messages.is_empty();
                            }
                            Err(e) => {
                                error!("Error retrieving missed messages in {}: {e:?}", g.id)
                            }
                        };
                    }
                    if finished {
                        config.save();
                        break;
                    }
                }
            }
        }
        drop(data);
        info!("Finished catching up with messages for guild {}.", g.id);
        ctx
    }

    pub async fn init(ctx: Context, g: Guild) {
        loop {
            let data = ctx.data.read().await;
            if let Some(memes) = get_memes(&data, &g.id) {
                let reset_time = memes.next_reset();
                info!("Next reset: {}", reset_time);
                drop(data);
                let now = Utc::now();
                let time_until_ping = reset_time
                    .checked_sub_days(Days::new(2))
                    .unwrap()
                    .signed_duration_since(now);
                if time_until_ping.num_seconds() > 0 {
                    info!(
                        "Sleeping for {}s until it's time to ping",
                        time_until_ping.num_seconds()
                    );
                    tokio::time::sleep(time_until_ping.to_std().unwrap()).await;
                    let data = ctx.data.read().await;
                    if let Some(memes) = get_memes(&data, &g.id) {
                        let channel = memes
                            .channel()
                            .to_channel(&ctx.http)
                            .await
                            .unwrap()
                            .guild()
                            .unwrap();
                        if memes.list().is_empty() {
                            channel
                                .send_message(&ctx.http, |m| {
                                    m.add_embed(|e| {
                                        e.description(
                                            "**No memes?**
Two days left! Perhaps time to post some?",
                                        )
                                        .image("https://media.tenor.com/ve60xH3hKrcAAAAC/no.gif")
                                        .colour(crate::COLOUR)
                                    })
                                })
                                .await
                                .ok();
                        }
                    }
                    drop(data);
                }
                let now = Utc::now();
                let time_until_reset = reset_time.signed_duration_since(now);
                if time_until_reset.num_seconds() > 0 {
                    info!(
                        "Sleeping for {}s until it's time to reset",
                        time_until_reset.num_seconds()
                    );
                    tokio::time::sleep(time_until_reset.to_std().unwrap()).await;
                    // in case the settings have changed during
                    // our long slumber, give the earlier checks
                    // another go:
                    continue;
                }
                let mut data = ctx.data.write().await;
                let config = data.get_mut::<Config>().unwrap();
                let guild = config.guild_mut(&g.id);
                if let Some(memes) = guild.memes_mut() {
                    if let Ok(channel) = memes.channel().to_channel(&ctx.http).await {
                        let channel = channel.guild().unwrap();
                        let mut most_reactions = 0;
                        let mut victor: Option<Message> = None;
                        let mut reacted = memes.has_reacted();
                        for meme in memes.list() {
                            if let Ok(meme) = channel.message(&ctx.http, meme).await {
                                if !reacted
                                    && rand::thread_rng().gen_bool(REACTION_CHANCE)
                                    && meme.react(&ctx.http, REACTION_EMOTE).await.is_ok()
                                {
                                    reacted = true;
                                }
                                let total_reactions: u64 =
                                    meme.reactions.iter().map(|m| m.count).sum();
                                if total_reactions > most_reactions {
                                    most_reactions = total_reactions;
                                    victor = Some(meme);
                                }
                            }
                        }

                        memes.reset();
                        if let Some(victor) = victor {
                            channel
                                .send_message(
                                    &ctx.http,
                                    crate::command::create_embed(format!(
                                        "**Voting results**
Congratulations {} for winning this week's meme contest, with \
their entry [here]({})!

It won with a resounding {most_reactions} votes.

I've reset the entries, so post your best memes and perhaps next \
week you'll win? 😉

You've got until {}.",
                                        victor.author.mention(),
                                        victor.link(),
                                        memes.next_reset().format(crate::DATE_FMT),
                                    )),
                                )
                                .await
                                .unwrap();
                        } else {
                            channel
                                .send_message(
                                    &ctx.http,
                                    crate::command::create_embed(format!(
                                        "**No votes**
There weren't any votes (reactions), so there's no winner. Sadge.

I've reset the entries, so can you, like, _do something_ this week?

You've got until {}.",
                                        memes.next_reset().format(crate::DATE_FMT)
                                    )),
                                )
                                .await
                                .unwrap();
                        }
                        config.save();
                    }
                }
                drop(data);
            } else {
                drop(data);
            }
            // let up for a while so we don't hog the mutex...
            // sleep for a day, which also gives a nice window to change
            // the reset time if we're sleeping after a proper reset.
            // if the memes channel is re-set during this day, then the
            // system will properly start using the new artifical time.
            // (eg, initially set up system at 10pm -> all resets occur
            // at 10pm. wait until reset, and if you then re-set the memes
            // channel at 7am the next morning, resets will start occuring
            // at 7am.)
            tokio::time::sleep(Duration::new(86_400, 0)).await;
        }
    }

    pub async fn message(ctx: &Context, message: &Message) {
        let mut data = ctx.data.write().await;
        let config = data.get_mut::<Config>().unwrap();
        let guild = config.guild_mut(&message.guild_id.unwrap());
        if let Some(memes) = guild.memes_mut() {
            if message.channel_id == memes.channel() && !message.is_own(&ctx.cache) {
                if !memes.has_reacted()
                    && rand::thread_rng().gen_bool(REACTION_CHANCE)
                    && message.react(&ctx.http, REACTION_EMOTE).await.is_ok()
                {
                    memes.reacted();
                }
                memes.add(message.id);
                config.save()
            }
        }
    }
}
