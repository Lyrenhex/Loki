use crate::command::{notify_subscribers, OptionType};
use crate::subsystems;
use crate::subsystems::events::Event;
use log::{error, info};
use serenity::model::prelude::GuildId;
use serenity::{
    async_trait,
    model::prelude::{
        command::{Command, CommandOptionType},
        interaction::Interaction,
        Activity, Guild, Message, Presence, Ready,
    },
    prelude::{Context, EventHandler},
};

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

        notify_subscribers(
            &ctx,
            Event::Startup,
            format!(
                "**Hey!**
I'm starting up with version [{}]({}/releases/tag/v{}). 😁",
                crate::VERSION,
                crate::GITHUB_URL,
                crate::VERSION,
            )
            .as_str(),
        )
        .await;

        notify_subscribers(&ctx, Event::Error, "test error!").await;
    }

    async fn guild_create(&self, ctx: Context, g: Guild, _is_new: bool) {
        let ctx = subsystems::memes::catch_up_messages(ctx, &g).await;

        tokio::spawn(subsystems::memes::init(ctx, g)).await.unwrap();
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
                        notify_subscribers(
                            &ctx,
                            Event::Error,
                            &format!(
                                "**Error running '{}':**
{e}",
                                cmd.name()
                            ),
                        )
                        .await;
                        crate::command::create_response(
                            &ctx.http,
                            &mut command,
                            &format!("{e}"),
                            false,
                        )
                        .await;
                    }
                    break;
                }
            }
        };
    }

    async fn message(&self, ctx: Context, message: Message) {
        subsystems::memes::message(&ctx, &message).await;
    }

    async fn presence_update(&self, ctx: Context, new_data: Presence) {
        info!("Handling Presence update for {}...", new_data.user.id);
        subsystems::stream_indicator::presence(&ctx, &new_data).await;
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
                    macro_rules! build_opt {
                        ($opt:ident) => {
                            |option| {
                                option
                                    .name($opt.name())
                                    .description($opt.description())
                                    .kind($opt.kind().into())
                                    .required($opt.required());
                                match $opt.kind() {
                                    OptionType::StringInput(min, max) => {
                                        if let Some(min) = min {
                                            option.min_length(min);
                                        }
                                        if let Some(max) = max {
                                            option.max_length(max);
                                        }
                                    }
                                    OptionType::StringSelect(options) => {
                                        options.iter().for_each(|s| {
                                            option.add_string_choice(s, s);
                                        });
                                    }
                                    OptionType::IntegerInput(min, max) => {
                                        if let Some(min) = min {
                                            option.min_int_value(min);
                                        }
                                        if let Some(max) = max {
                                            option.max_int_value(max);
                                        }
                                    }
                                    OptionType::IntegerSelect(options) => {
                                        options.iter().for_each(|s| {
                                            option.add_int_choice(s, *s as i32);
                                        });
                                    }
                                    OptionType::NumberInput(min, max) => {
                                        if let Some(min) = min {
                                            option.min_number_value(min);
                                        }
                                        if let Some(max) = max {
                                            option.max_number_value(max);
                                        }
                                    }
                                    OptionType::NumberSelect(options) => {
                                        options.iter().for_each(|s| {
                                            option.add_number_choice(s, *s);
                                        });
                                    }
                                    OptionType::Channel(types) => {
                                        if let Some(types) = types {
                                            option.channel_types(&types);
                                        }
                                    }
                                    OptionType::Boolean
                                    | OptionType::User
                                    | OptionType::Role
                                    | OptionType::Mentionable
                                    | OptionType::Attachment => {}
                                }
                                option
                            }
                        };
                    }

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
                                        subcmd = subcmd.create_sub_option(build_opt!(opt))
                                    }
                                    subcmd
                                })
                            }
                            for opt in cmd.options() {
                                command = command.create_option(build_opt!(opt))
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
