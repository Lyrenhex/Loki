use std::{collections::HashMap, time::Duration};

use chrono::{Days, Utc};
use log::{debug, error, info, trace, warn};
use rand::Rng;
use serde::{Deserialize, Serialize};
use serenity::{
    all::{
        ChannelId, ChannelType, CreateEmbed, CreateMessage, EditMessage, GetMessages, Guild,
        Message, MessageFlags, MessageId,
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
                    Box::pin(async move {
                        let channel_id = *get_param!(params, Channel, "channel");
                        let channel =
                            if let Some(channel) = channel_id.to_channel(&ctx).await?.guild() {
                                channel
                            } else {
                                return Err(Error::InvalidChannel);
                            };
                        let mut initial_message = channel
                            .send_message(
                                &ctx,
                                crate::command::create_embed(
                                    "Setting up meme subsystem...".to_string(),
                                ),
                            )
                            .await?;
                        let mut data = crate::acquire_data_handle!(write ctx);
                        let config = data.get_mut::<Config>().unwrap();
                        let guild_config = config.guild_mut(&command.guild_id.unwrap());
                        guild_config.set_memes_channel(Some((channel_id, initial_message.id)));
                        let reset_time = guild_config.memes().unwrap().next_reset();
                        config.save();
                        crate::drop_data_handle!(data);
                        let resp = format!("Memes channel set to {}.", channel);
                        initial_message
                            .edit(
                                &ctx,
                                EditMessage::new().embeds(vec![create_raw_embed(format!(
                                    "**Post your best memes!**
Vote by reacting to your favourite memes.
The post with the most total reactions by <t:{}:F> wins!",
                                    reset_time.timestamp(),
                                ))]),
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
                Box::pin(async move {
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
                        if let Some(channel) = channel.to_channel(&ctx).await?.guild() {
                            channel
                                .send_message(
                                    &ctx,
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
                Box::pin(async move {
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
                                    .to_user(&ctx)
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
                        && message.react(&ctx, REACTION_EMOTE).await.is_ok()
                    {
                        memes.reacted();
                    }
                    config.save()
                }
            }
            crate::drop_data_handle!(data);
        }
    }
}

impl MemesVoting {
    pub async fn get_messages(ctx: &Context, g: &Guild) -> Result<Vec<Message>, Error> {
        // Retrieve all meme messages for the week
        let data = crate::acquire_data_handle!(read ctx);
        let mut message_list = Vec::new();
        if let Some(memes) = get_memes(&data, &g.id) {
            let channel = memes.channel();
            let initial_message = *memes.initial_message();
            crate::drop_data_handle!(data);

            message_list.push(channel.message(&ctx, initial_message).await?);
            let mut finished = true;
            loop {
                if let Some(last_message) = message_list.last() {
                    let mut messages: Vec<Message> = channel
                        .messages(&ctx, GetMessages::default().after(last_message).limit(100))
                        .await?;
                    // The returned messages are **most recent first**.
                    // It's easier for us the other way around, so reverse it.
                    messages.reverse();
                    finished = messages.is_empty();
                    message_list.append(&mut messages);
                };
                if finished {
                    break;
                }
                trace!(
                    "Entries: {} (last: {:?})",
                    message_list.len(),
                    message_list.last().map(|m| m.id)
                );
            }
        }
        message_list.retain(|m| !m.is_own(&ctx.cache));
        Ok(message_list)
    }

    pub async fn process_memes(ctx: &Context, g: &Guild) -> Result<(), Error> {
        let time = Utc::now();
        let mut meme_list = Self::get_messages(ctx, g).await?;
        let mut data = crate::acquire_data_handle!(write ctx);
        let config = data.get_mut::<Config>().unwrap();
        let guild = config.guild_mut(&g.id);
        if let Some(memes) = guild.memes_mut() {
            let channel = memes.channel().to_channel(&ctx).await?;
            let channel = channel.guild().unwrap();
            let reacted = memes.has_reacted();
            crate::drop_data_handle!(data);
            info!("[Guild: {}] Processing {} entries.", &g.id, meme_list.len());
            debug!("[Guild: {}] Entries: {:?}", &g.id, meme_list);
            let mut initial_message = channel
                .send_message(
                    &ctx,
                    crate::command::create_embed(format!(
                        "Processing {} results...",
                        meme_list.len(),
                    )),
                )
                .await?;
            if !reacted && !meme_list.is_empty() {
                let i = rand::thread_rng().gen_range(0..meme_list.len());
                info!(
                    "[Guild: {}] Reacting to random meme #{i} ({:?})",
                    &g.id,
                    meme_list.get(i)
                );
                if let Err(e) = meme_list.get(i).unwrap().react(&ctx, REACTION_EMOTE).await {
                    error!(
                        "[Guild: {}] Error reacting to random meme #{i} ({:?}): {e:?}",
                        &g.id,
                        meme_list.get(i),
                    );
                    notify_subscribers(
                        ctx,
                        Event::Error,
                        &format!(
                            "[Guild: {}] Error reacting to random meme #{i:?}: {e}",
                            &g.id
                        ),
                    )
                    .await;
                } else {
                    let mut data = crate::acquire_data_handle!(write ctx);
                    let config = data.get_mut::<Config>().unwrap();
                    let guild = config.guild_mut(&g.id);
                    let memes = guild.memes_mut().unwrap();
                    memes.reacted();
                    config.save();
                    crate::drop_data_handle!(data);
                    meme_list = Self::get_messages(ctx, g).await?;
                }
            }
            let mut data = crate::acquire_data_handle!(write ctx);
            let config = data.get_mut::<Config>().unwrap();
            let guild = config.guild_mut(&g.id);
            let memes = guild.memes_mut().unwrap();
            memes.reset(time, initial_message.id);
            let next_reset = memes.next_reset().timestamp();
            crate::drop_data_handle!(data);
            let new_text = if !meme_list.is_empty() {
                // Reverse sort the meme list by number of votes.
                // Unstable sorting means that if two memes have the same number of votes, then it is not generally predictable which meme will win (it is not 'first one wins').
                // However, order of votes should be accurate nonetheless.
                meme_list.sort_unstable_by(|a, b| {
                    b.reactions
                        .iter()
                        .map(|m| m.count)
                        .sum::<u64>()
                        .cmp(&a.reactions.iter().map(|m| m.count).sum::<u64>())
                });
                let victor = meme_list.first().unwrap();
                let most_reactions: u64 = victor.reactions.iter().map(|m| m.count).sum();
                if most_reactions > 0 {
                    let mut data = crate::acquire_data_handle!(write ctx);
                    let config = data.get_mut::<Config>().unwrap();
                    let guild = config.guild_mut(&g.id);
                    let memes = guild.memes_mut().unwrap();
                    memes.add_victory(victor.author.id);
                    crate::drop_data_handle!(data);
                    info!(
                        "[Guild: {}] Registered victory for {} ({}) with message ID {} ({} votes)",
                        &g.id, victor.author.name, victor.author.id, victor.id, most_reactions
                    );
                    format!(
                        "**Voting results**
Congratulations {} for winning this week's meme contest, with \
their entry [here]({})!

It won with a resounding {most_reactions} votes.

I've reset the entries, so post your best memes and perhaps next \
week you'll win? 😉

You've got until <t:{next_reset}:F>.",
                        victor.author.mention(),
                        victor.link(),
                    )
                } else {
                    info!("[Guild: {}] Memes processed with no votes at all.", &g.id);
                    format!(
                        "**No votes**
There weren't any votes (reactions), so there's no winner. Sadge.

I've reset the entries, so can you, like, _make a decision_ this time?

You've got until <t:{next_reset}:F>.",
                    )
                }
            } else {
                info!("[Guild: {}] No memes to process...", &g.id);
                format!(
                    "**No entries**
There weren't any entries. You know you can't win if you don't enter, right?

I've reset the entries, so can you, like, _do something_ this week?

You've got until <t:{next_reset}:F>.",
                )
            };
            loop {
                if initial_message
                    .edit(
                        &ctx,
                        EditMessage::new()
                            .embeds(vec![crate::command::create_raw_embed(&new_text)]),
                    )
                    .await
                    .is_ok()
                {
                    break;
                }
                // It failed; we should wait for a few minutes before trying again!
                tokio::time::sleep(Duration::new(300, 0)).await;
            }
        } else {
            crate::drop_data_handle!(data);
        }
        let mut data = crate::acquire_data_handle!(write ctx);
        let config = data.get_mut::<Config>().unwrap();
        config.save();
        crate::drop_data_handle!(data);
        Ok(())
    }

    pub async fn memes_process_iter(ctx: &Context, g: &Guild) -> Result<(), Error> {
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
                        .to_channel(&ctx)
                        .await
                        .unwrap()
                        .guild()
                        .unwrap();
                    crate::drop_data_handle!(data);
                    if Self::get_messages(ctx, g).await?.is_empty() {
                        channel
                            .send_message(
                                &ctx,
                                CreateMessage::new().add_embed(
                                    CreateEmbed::new()
                                        .description(format!(
                                            "**No memes?**
<t:{}:R> left! Perhaps time to post some?",
                                            reset_time.timestamp()
                                        ))
                                        .image(NO_MEMES_GIF)
                                        .colour(crate::COLOUR),
                                ),
                            )
                            .await?;
                    }
                } else {
                    crate::drop_data_handle!(data);
                }
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
            }
            Self::process_memes(ctx, g).await
        } else {
            crate::drop_data_handle!(data);
            Ok(())
        }
    }

    pub async fn guild_init(ctx: Context, g: Guild) {
        loop {
            if let Err(e) = Self::memes_process_iter(&ctx, &g).await {
                if let Error::SerenityError(serenity::Error::Http(
                    serenity::all::HttpError::Request(_),
                )) = e
                {
                    warn!("[Guild: {}] HTTP request error in memes processing thread (do we have network?): {e:?}", &g.id);
                } else {
                    notify_subscribers(
                        &ctx,
                        Event::Error,
                        &format!(
                            "[Guild: {}] Unexpected error in memes processing thread: {e:?}",
                            &g.id
                        ),
                    )
                    .await;
                    error!(
                        "[Guild: {}] Unexpected error in memes processing thread: {e:?}",
                        &g.id
                    );
                }
            }
            tokio::time::sleep(Duration::new(300, 0)).await;
        }
    }
}

#[derive(Deserialize, Serialize, Clone)]
pub struct Memes {
    channel: ChannelId,
    last_reset: chrono::DateTime<Utc>,
    initial_message: MessageId,
    times_won: HashMap<String, u32>,
    reacted: bool,
}

impl Memes {
    pub fn new(channel: ChannelId, initial_message: MessageId) -> Self {
        Self {
            channel,
            last_reset: Utc::now(),
            initial_message,
            times_won: HashMap::new(),
            reacted: false,
        }
    }

    pub fn next_reset(&self) -> chrono::DateTime<Utc> {
        self.last_reset.checked_add_days(Days::new(7)).unwrap()
    }

    pub fn reset(&mut self, time: chrono::DateTime<Utc>, initial_message: MessageId) {
        self.last_reset = time;
        self.reacted = false;
        self.initial_message = initial_message;
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

    pub fn initial_message(&self) -> &MessageId {
        &self.initial_message
    }
}
