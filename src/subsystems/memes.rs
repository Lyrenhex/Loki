use std::{collections::HashMap, time::Duration};

use chrono::{Days, Utc};
use log::{error, info};
use rand::Rng;
use serde::{Deserialize, Serialize};
use serenity::{
    all::{
        CacheHttp, ChannelId, ChannelType, CreateEmbed, CreateMessage, GetMessages, Guild, Message,
        MessageFlags, MessageId,
    },
    async_trait, futures,
    model::{id::UserId, Permissions},
    prelude::{Context, Mentionable},
};

use crate::{
    command::{create_embed, Command, PermissionType},
    config::get_memes,
    create_raw_embed, ActionResponse, Error,
};
use crate::{
    command::{notify_subscribers, OptionType},
    config::Config,
    subsystems::events::Event,
};

use super::Subsystem;

const REACTION_CHANCE: f64 = 0.1;
const REACTION_EMOTE: char = '🤖';
const NO_MEMES_GIF: &str = "https://media.tenor.com/ve60xH3hKrcAAAAC/no.gif";

pub struct MemesVoting;

#[async_trait]
impl Subsystem for MemesVoting {
    fn generate_commands(&self) -> Vec<Command<'static>> {
        vec![Command::new(
            "memes",
            "Commands for the meme-voting system.",
            PermissionType::ServerPerms(Permissions::USE_APPLICATION_COMMANDS),
            None,
        )
        .add_variant(
            Command::new(
                "set_channel",
                "Sets the memes channel for this server and initialises the meme subsystem.",
                PermissionType::ServerPerms(Permissions::MANAGE_CHANNELS),
                Some(Box::new(move |ctx, command, params| {
                    Box::pin(async {
                        let channel_id = *get_param!(params, Channel, "channel");
                        let channel = if let Some(channel) =
                            channel_id.to_channel(&ctx.http()).await?.guild()
                        {
                            channel
                        } else {
                            return Err(Error::InvalidChannel);
                        };
                        let mut data = crate::acquire_data_handle!(write ctx);
                        let config = data.get_mut::<Config>().unwrap();
                        let guild_config = config.guild_mut(&command.guild_id.unwrap());
                        guild_config.set_memes_channel(Some(channel_id));
                        let reset_time = guild_config.memes().unwrap().next_reset();
                        config.save();
                        crate::drop_data_handle!(data);
                        let resp = format!("Memes channel set to {}.", channel);
                        channel
                            .send_message(
                                &ctx.http(),
                                create_embed(format!(
                                    "**Post your best memes!**
Vote by reacting to your favourite memes.
The post with the most total reactions by <t:{}:F> wins!",
                                    reset_time.timestamp(),
                                )),
                            )
                            .await?;
                        Ok(Some(ActionResponse::new(create_raw_embed(&resp), true)))
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
            Some(Box::new(move |ctx, command, _params| {
                Box::pin(async {
                    let mut data = crate::acquire_data_handle!(write ctx);
                    let config = data.get_mut::<Config>().unwrap();
                    let channel = config
                        .guild_mut(&command.guild_id.unwrap())
                        .memes()
                        .map(|memes| memes.channel());
                    config
                        .guild_mut(&command.guild_id.unwrap())
                        .set_memes_channel(None);
                    config.save();
                    crate::drop_data_handle!(data);
                    let resp = "Memes channel unset.".to_string();
                    if let Some(channel) = channel {
                        if let Some(channel) = channel.to_channel(&ctx.http()).await?.guild() {
                            channel
                                .send_message(
                                    &ctx.http(),
                                    create_embed(
                                        "**Halt your memes!**
I won't see them anymore. :("
                                            .to_string(),
                                    ),
                                )
                                .await?;
                        }
                    }
                    Ok(Some(ActionResponse::new(create_raw_embed(&resp), true)))
                })
            })),
        ))
        .add_variant(Command::new(
            "leaderboard",
            "Display the leaderboard for meme voting victories.",
            PermissionType::ServerPerms(Permissions::USE_APPLICATION_COMMANDS),
            Some(Box::new(move |ctx, command, _params| {
                Box::pin(async {
                    let mut users = String::new();
                    let mut counts = String::new();
                    let data = crate::acquire_data_handle!(read ctx);
                    if let Some(memes) = get_memes(&data, &command.guild_id.unwrap()) {
                        let mut entries = memes
                            .victors()
                            .iter()
                            .map(|(uid, count)| (uid.clone(), *count))
                            .collect::<Vec<(String, u32)>>();
                        entries.sort_unstable_by(|(_, cnt_a), (_, cnt_b)| cnt_b.cmp(cnt_a));
                        let iter = entries.iter().take(10);
                        users = futures::future::try_join_all(iter.clone().map(|(uid, _)| async {
                            Ok::<String, crate::Error>(
                                UserId::from(uid.parse::<u64>().unwrap())
                                    .to_user(&ctx.http())
                                    .await?
                                    .mention()
                                    .to_string(),
                            )
                        }))
                        .await?
                        .join("\n");
                        counts = iter
                            .clone()
                            .map(|(_, cnt)| cnt.to_string())
                            .collect::<Vec<String>>()
                            .join("\n");
                    }
                    let resp = create_raw_embed("**Top 10 Memesters**".to_string())
                        .field("User", users, true)
                        .field("Victories", counts, true);
                    Ok(Some(ActionResponse::new(resp, false)))
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
            let mut data = crate::acquire_data_handle!(write ctx);
            let config = data.get_mut::<Config>().unwrap();
            let guild = config.guild_mut(&guild);
            if let Some(memes) = guild.memes_mut() {
                if message.channel_id == memes.channel() && !message.is_own(&ctx.cache) {
                    if !memes.has_reacted()
                        && rand::thread_rng().gen_bool(REACTION_CHANCE)
                        && message.react(&ctx.http(), REACTION_EMOTE).await.is_ok()
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
        let mut data = crate::acquire_data_handle!(write ctx);
        info!("Catching up with messages for guild {}...", g.id);
        let config = data.get_mut::<Config>().unwrap();
        let guild = config.guild_mut(&g.id);
        // TODO: we should really split this into two stages to limit the amount of time we have a writable handle on this - retrieve messages via Discord API using a read handle into a local vector, then grab a write handle and actually do the modifications we need.
        if let Some(memes) = guild.memes_mut() {
            // catch up on any messages that were missed while we were offline.
            if let Ok(channel) = memes.channel().to_channel(&ctx.http()).await {
                let channel = channel.guild().unwrap();
                loop {
                    if let Some(last_message) = memes.list().last() {
                        match channel
                            .messages(
                                &ctx.http(),
                                GetMessages::default().after(last_message).limit(100),
                            )
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
        crate::drop_data_handle!(data);
        info!("Finished catching up with messages for guild {}.", g.id);
        ctx
    }

    pub async fn guild_init(ctx: Context, g: Guild) {
        let ctx = Self::catch_up_messages(ctx, &g).await;

        loop {
            let data = crate::acquire_data_handle!(read ctx);
            if let Some(memes) = get_memes(&data, &g.id) {
                let reset_time = memes.next_reset();
                info!("[Guild: {}] Next reset: {}", &g.id, reset_time);
                crate::drop_data_handle!(data);
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
                    let data = crate::acquire_data_handle!(read ctx);
                    if let Some(memes) = get_memes(&data, &g.id) {
                        let channel = memes
                            .channel()
                            .to_channel(&ctx.http())
                            .await
                            .unwrap()
                            .guild()
                            .unwrap();
                        if memes.list().len() - 1 == 0 {
                            channel
                                .send_message(
                                    &ctx.http(),
                                    CreateMessage::new().add_embed(
                                        CreateEmbed::new()
                                            .description(
                                                "**No memes?**
Two days left! Perhaps time to post some?",
                                            )
                                            .image(NO_MEMES_GIF)
                                            .colour(crate::COLOUR),
                                    ),
                                )
                                .await
                                .ok();
                        }
                    }
                    crate::drop_data_handle!(data);
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
                let mut data = crate::acquire_data_handle!(write ctx);
                let config = data.get_mut::<Config>().unwrap();
                let guild = config.guild_mut(&g.id);
                if let Some(memes) = guild.memes_mut() {
                    if let Ok(channel) = memes.channel().to_channel(&ctx.http()).await {
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
                            if let Ok(meme) = channel.message(&ctx.http(), meme).await {
                                if !meme.is_own(&ctx.cache) {
                                    let mut total_reactions: u64 =
                                        meme.reactions.iter().map(|m| m.count).sum();
                                    if !reacted
                                        && rand::thread_rng().gen_bool(REACTION_CHANCE)
                                        && meme.react(&ctx.http(), REACTION_EMOTE).await.is_ok()
                                    {
                                        reacted = true;
                                        total_reactions += 1;
                                    }
                                    if total_reactions > most_reactions {
                                        most_reactions = total_reactions;
                                        victor = Some(meme);
                                    }
                                }
                            }
                        }

                        let initial_message = if let Some(victor) = victor {
                            memes.add_victory(victor.author.id);
                            channel
                                .send_message(
                                    &ctx.http(),
                                    crate::command::create_embed(format!(
                                        "**Voting results**
Congratulations {} for winning this week's meme contest, with \
their entry [here]({})!

It won with a resounding {most_reactions} votes.

I've reset the entries, so post your best memes and perhaps next \
week you'll win? 😉

You've got until <t:{}:F>.",
                                        victor.author.mention(),
                                        victor.link(),
                                        memes.next_reset().timestamp(),
                                    )),
                                )
                                .await
                                .unwrap()
                        } else {
                            channel
                                .send_message(
                                    &ctx.http(),
                                    crate::command::create_embed(format!(
                                        "**No votes**
There weren't any votes (reactions), so there's no winner. Sadge.

I've reset the entries, so can you, like, _do something_ this week?

You've got until <t:{}:F>.",
                                        memes.next_reset().timestamp()
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
                crate::drop_data_handle!(data);
            } else {
                crate::drop_data_handle!(data);
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
    times_won: HashMap<String, u32>,
    reacted: bool,
}

impl Memes {
    pub fn new(channel: ChannelId) -> Self {
        Self {
            channel,
            last_reset: Utc::now(),
            memes_list: Vec::new(),
            times_won: HashMap::new(),
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

    pub fn victors(&self) -> &HashMap<String, u32> {
        &self.times_won
    }

    pub fn add_victory(&mut self, uid: UserId) {
        *self.times_won.entry(uid.to_string()).or_insert(0) += 1;
    }
}
