use log::{error, info};
use serenity::model::prelude::GuildId;
use serenity::{
    async_trait,
    model::prelude::{
        command::{Command, CommandOptionType},
        interaction::Interaction,
        Activity, ActivityType, Guild, Message, Presence, Ready,
    },
    prelude::{Context, EventHandler},
};

use crate::config::Config;

// guild to use for testing purposes.
#[cfg(debug_assertions)]
const DEBUG_GUILD_ID: &str = env!("LOKI_DEBUG_GUILD_ID");

const STREAMING_PREFIX: &str = "🔴 ";

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
        let ctx = crate::subsystems::Memes::catch_up_messages(ctx, &g).await;

        tokio::spawn(crate::subsystems::Memes::init(ctx, g))
            .await
            .unwrap();
    }

    async fn interaction_create(&self, ctx: Context, interaction: Interaction) {
        if let Interaction::ApplicationCommand(mut command) = interaction {
            for cmd in self.commands.iter() {
                if cmd.name() == command.data.name {
                    let mut cmd = cmd;
                    if !command.data.options.is_empty()
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
        crate::subsystems::Memes::message(&ctx, &message).await;
    }

    async fn presence_update(&self, ctx: Context, new_data: Presence) {
        info!("Handling Presence update for {}...", new_data.user.id);
        let data = ctx.data.read().await;
        let config = data.get::<Config>().unwrap();
        if new_data
            .activities
            .iter()
            .any(|a| a.kind == ActivityType::Streaming)
        {
            if let Some(user) = new_data.user.to_user() {
                for guild in config.guilds().map(|g| GuildId(g.parse::<u64>().unwrap())) {
                    let user = user.clone();
                    let nick = user.nick_in(&ctx.http, guild).await.unwrap_or(user.name);
                    if !nick.starts_with(STREAMING_PREFIX) {
                        // the user is streaming, but they aren't marked as such.
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
                let user = user.clone();
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
            let guild = GuildId(DEBUG_GUILD_ID.parse::<u64>().unwrap_or_else(|_| {
                panic!(
                    "{}",
                    ("Couldn't parse 'LOKI_DEBUG_GUILD_ID' as a u64: ".to_owned() + DEBUG_GUILD_ID)
                )
            }))
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
