use std::time::Duration;

use chrono::{Days, Utc};
use log::{error, info};
#[cfg(debug_assertions)]
use serenity::model::prelude::GuildId;
use serenity::{
    async_trait,
    model::prelude::{
        command::{Command, CommandOptionType},
        interaction::Interaction,
        Activity, Guild, Message, Ready,
    },
    prelude::{Context, EventHandler, Mentionable},
};

use crate::{config::Config, COLOUR};

// guild to use for testing purposes.
#[cfg(debug_assertions)]
const DEBUG_GUILD_ID: &str = env!("LOKI_DEBUG_GUILD_ID");

/// Core implementation logic for [serenity] events.
pub struct SerenityHandler<'a> {
    commands: Vec<crate::command::Command<'a>>,
}

#[async_trait]
impl EventHandler for SerenityHandler<'_> {
    async fn ready(&self, ctx: Context, ready: Ready) {
        info!("Loki is connected as {}", ready.user.name);

        ctx.set_activity(Activity::playing("tricks")).await;

        // creates the global application commands
        self.create_commands(&ctx).await;
    }

    async fn guild_create(&self, ctx: Context, g: Guild, _is_new: bool) {
        tokio::spawn(async move {
            loop {
                let data = ctx.data.read().await;
                let config = data.get::<Config>().unwrap();
                let guild = config.guild(&g.id);
                if let Some(guild) = guild {
                    if let Some(memes) = guild.memes() {
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
                            let config = data.get::<Config>().unwrap();
                            let guild = config.guild(&g.id);
                            if let Some(guild) = guild {
                                if let Some(memes) = guild.memes() {
                                    let channel = memes
                                        .channel()
                                        .to_channel(&ctx.http)
                                        .await
                                        .unwrap()
                                        .guild()
                                        .unwrap();
                                    if memes.list().len() == 0 {
                                        channel
                                            .send_message(&ctx.http, |m| {
                                                m.add_embed(|e| {
                                                    e.description(
                                                "**No memes?**
Two days left! Perhaps time to post some?",
                                            )
                                            .image(
                                                "https://media.tenor.com/ve60xH3hKrcAAAAC/no.gif",
                                            )
                                            .colour(COLOUR)
                                                })
                                            })
                                            .await
                                            .unwrap();
                                    }
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
                            let channel = memes
                                .channel()
                                .to_channel(&ctx.http)
                                .await
                                .unwrap()
                                .guild()
                                .unwrap();
                            let mut most_reactions = 0;
                            let mut victor: Option<Message> = None;
                            for meme in memes.list() {
                                if let Ok(meme) = channel.message(&ctx.http, meme).await {
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
                                    .send_message(&ctx.http, |m| {
                                        m.add_embed(|e| {
                                            e.description(format!(
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
                                            ))
                                            .colour(COLOUR)
                                        })
                                    })
                                    .await
                                    .unwrap();
                            } else {
                                channel
                                    .send_message(&ctx.http, |m| {
                                        m.add_embed(|e| {
                                            e.description(format!(
                                                "**No votes**
There weren't any votes (reactions), so there's no winner. Sadge.

I've reset the entries, so can you, like, _do something_ this week?

You've got until {}.",
                                                memes.next_reset().format(crate::DATE_FMT)
                                            ))
                                            .colour(COLOUR)
                                        })
                                    })
                                    .await
                                    .unwrap();
                            }
                            config.save();
                        }
                        drop(data);
                    } else {
                        drop(data);
                    }
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
        })
        .await
        .unwrap();
    }

    async fn interaction_create(&self, ctx: Context, interaction: Interaction) {
        if let Interaction::ApplicationCommand(mut command) = interaction {
            for cmd in self.commands.iter() {
                if cmd.name() == command.data.name {
                    let mut cmd = cmd;
                    if command.data.options.len() > 0
                        && command.data.options[0].kind == CommandOptionType::SubCommand
                    {
                        for subcmd in cmd.variants() {
                            if subcmd.name() == command.data.options[0].name {
                                cmd = subcmd;
                                break;
                            }
                        }
                    };
                    if let Err(e) = cmd.run(&ctx, &mut command).await {
                        error!("Error running '{}': {e:?}", cmd.name());
                        crate::command::create_response(&ctx.http, &mut command, &format!("{e}"))
                            .await;
                    }
                    break;
                }
            }
        };
    }

    async fn message(&self, ctx: Context, message: Message) {
        let mut data = ctx.data.write().await;
        let config = data.get_mut::<Config>().unwrap();
        let guild = config.guild_mut(&message.guild_id.unwrap());
        if let Some(memes) = guild.memes_mut() {
            if message.channel_id == memes.channel() && !message.is_own(&ctx.cache) {
                memes.add(message.id);
                config.save()
            }
        }
        drop(data);
    }
}

impl<'a> SerenityHandler<'a> {
    /// Construct a new handler from a populated config.
    pub fn new(commands: Vec<crate::command::Command<'a>>) -> Self {
        Self { commands }
    }

    async fn create_commands(&self, ctx: &Context) -> Vec<Command> {
        macro_rules! command_constructor {
            () => {
                |mut commands| {
                    for cmd in self.commands.iter() {
                        commands = commands.create_application_command(|command| {
                            let mut command = command
                                .name(cmd.name())
                                .description(cmd.description())
                                .dm_permission(
                                    *cmd.permissions() == crate::command::PermissionType::Universal,
                                );
                            if let crate::command::PermissionType::ServerPerms(permissions) =
                                *cmd.permissions()
                            {
                                command = command.default_member_permissions(permissions);
                            }
                            for variant in cmd.variants() {
                                command = command.create_option(|subcmd| {
                                    let mut subcmd = subcmd
                                        .name(variant.name())
                                        .description(variant.description())
                                        .kind(CommandOptionType::SubCommand)
                                        .required(false);
                                    for opt in variant.options() {
                                        subcmd = subcmd.create_sub_option(|option| {
                                            option
                                                .name(opt.name())
                                                .description(opt.description())
                                                .kind(opt.kind())
                                                .required(opt.required())
                                        })
                                    }
                                    subcmd
                                })
                            }
                            for opt in cmd.options() {
                                command = command.create_option(|option| {
                                    option
                                        .name(opt.name())
                                        .description(opt.description())
                                        .kind(opt.kind())
                                        .required(opt.required())
                                })
                            }
                            command
                        })
                    }
                    commands
                }
            };
        }
        #[cfg(debug_assertions)]
        {
            let guild = GuildId(
                DEBUG_GUILD_ID.parse::<u64>().expect(
                    ("Couldn't parse 'LOKI_DEBUG_GUILD_ID' as a u64: ".to_owned() + DEBUG_GUILD_ID)
                        .as_str(),
                ),
            )
            .to_partial_guild(&ctx.http)
            .await
            .unwrap();
            guild
                .set_application_commands(&ctx.http, command_constructor!())
                .await
                .expect("Failed to create command")
        }
        #[cfg(not(debug_assertions))]
        {
            Command::set_global_application_commands(&ctx.http, command_constructor!())
                .await
                .expect("Failed to create command")
        }
    }
}
