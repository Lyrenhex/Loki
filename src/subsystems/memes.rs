use std::time::Duration;

use chrono::{Days, Utc};
use log::{error, info};
use rand::Rng;
use serde::{Deserialize, Serialize};
use serenity::{
    async_trait,
    model::{
        prelude::{
            interaction::application_command::CommandDataOptionValue, ChannelId, ChannelType,
            Guild, Message, MessageFlags, MessageId,
        },
        Permissions,
    },
    prelude::{Context, Mentionable},
};

use crate::{
    command::{create_embed, create_response, Command, PermissionType},
    config::get_memes,
};
use crate::{
    command::{notify_subscribers, OptionType},
    config::Config,
    subsystems::events::Event,
};

use super::Subsystem;

const DATE_FMT: &str = "%l:%M%P on %A %e %B %Y (UTC%Z)";
const REACTION_CHANCE: f64 = 0.1;
const REACTION_EMOTE: char = '🤖';

pub struct MemesVoting;

#[async_trait]
impl Subsystem for MemesVoting {
    fn generate_commands(&self) -> Vec<Command<'static>> {
        vec![Command::new(
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
                                    reset_time.with_timezone(&chrono::Local).format(DATE_FMT),
                                )),
                            )
                            .await?;
                        create_response(&ctx.http, command, &resp, true).await;
                        Ok(())
                    })
                })),
            )
            .add_option(crate::command::Option::new(
                "channel",
                "The channel which is to be used for memes.",
                OptionType::Channel(Some(vec![ChannelType::Text])),
                true,
            )),
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
                    create_response(&ctx.http, command, &resp, true).await;
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
        ))]
    }

    async fn message(&self, ctx: &Context, message: &Message) {
        if let Some(flags) = message.flags {
            if flags.contains(MessageFlags::EPHEMERAL) {
                return;
            }
        }
        if let Some(guild) = message.guild_id {
            let mut data = ctx.data.write().await;
            let config = data.get_mut::<Config>().unwrap();
            let guild = config.guild_mut(&guild);
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
}

impl MemesVoting {
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
                                notify_subscribers(
                                    &ctx,
                                    Event::Error,
                                    format!("Error retrieving missed messages in {}: {e:?}", g.id)
                                        .as_str(),
                                )
                                .await;
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

    pub async fn guild_init(ctx: Context, g: Guild) {
        let ctx = Self::catch_up_messages(ctx, &g).await;

        loop {
            let data = ctx.data.read().await;
            if let Some(memes) = get_memes(&data, &g.id) {
                let reset_time = memes.next_reset();
                info!("[Guild: {}] Next reset: {}", &g.id, reset_time);
                drop(data);
                let now = Utc::now();
                let time_until_ping = reset_time
                    .checked_sub_days(Days::new(2))
                    .unwrap()
                    .signed_duration_since(now);
                if time_until_ping.num_seconds() > 0 {
                    info!(
                        "[Guild: {}] Sleeping for {}s until it's time to ping",
                        &g.id,
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
                info!(
                    "[Guild: {}] Time until reset: {}",
                    &g.id,
                    time_until_reset.num_seconds()
                );
                if time_until_reset.num_seconds() > 0 {
                    info!(
                        "[Guild: {}] Sleeping for {}s until it's time to reset",
                        &g.id,
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
                        let meme_list = memes.reset();
                        info!(
                            "[Guild: {}] Performing reset on {} entries.",
                            &g.id,
                            meme_list.len()
                        );
                        for meme in meme_list {
                            if let Ok(meme) = channel.message(&ctx.http, meme).await {
                                if !meme.is_own(&ctx.cache) {
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
                        }

                        let initial_message = if let Some(victor) = victor {
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
                                        memes
                                            .next_reset()
                                            .with_timezone(&chrono::Local)
                                            .format(DATE_FMT),
                                    )),
                                )
                                .await
                                .unwrap()
                        } else {
                            channel
                                .send_message(
                                    &ctx.http,
                                    crate::command::create_embed(format!(
                                        "**No votes**
There weren't any votes (reactions), so there's no winner. Sadge.

I've reset the entries, so can you, like, _do something_ this week?

You've got until {}.",
                                        memes
                                            .next_reset()
                                            .with_timezone(&chrono::Local)
                                            .format(DATE_FMT)
                                    )),
                                )
                                .await
                                .unwrap()
                        };
                        memes.add(initial_message.id);
                        config.save();
                        info!("[Guild: {}] Reset complete.", &g.id);
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
}

#[derive(Deserialize, Serialize, Clone)]
pub struct Memes {
    channel: ChannelId,
    last_reset: chrono::DateTime<Utc>,
    memes_list: Vec<MessageId>,
    reacted: bool,
}

impl Memes {
    pub fn new(channel: ChannelId) -> Self {
        Self {
            channel,
            last_reset: Utc::now(),
            memes_list: Vec::new(),
            reacted: false,
        }
    }

    pub fn list(&self) -> &Vec<MessageId> {
        &self.memes_list
    }

    pub fn add(&mut self, message: MessageId) {
        self.memes_list.push(message);
    }

    pub fn next_reset(&self) -> chrono::DateTime<Utc> {
        self.last_reset.checked_add_days(Days::new(7)).unwrap()
    }

    pub fn reset(&mut self) -> Vec<MessageId> {
        self.last_reset = Utc::now();
        self.reacted = false;
        let memes_list = self.memes_list.clone();
        self.memes_list.clear();
        memes_list
    }

    pub fn channel(&self) -> ChannelId {
        self.channel
    }

    pub fn has_reacted(&self) -> bool {
        self.reacted
    }

    pub fn reacted(&mut self) {
        self.reacted = true;
    }
}
