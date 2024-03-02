use crate::command::OptionType;
use crate::config::Config;
use crate::subsystems;
use log::{error, info};
use serenity::builder::{CreateApplicationCommand, CreateApplicationCommandOption};
#[cfg(debug_assertions)]
use serenity::model::prelude::GuildId;
use serenity::model::prelude::{GuildChannel, Member};
use serenity::{
    async_trait,
    model::prelude::{
        command::{Command, CommandOptionType},
        interaction::Interaction,
        Activity, Guild, Message, Presence, Ready,
    },
    prelude::{Context, EventHandler},
};
use tokio::task::JoinSet;

#[cfg(feature = "events")]
use crate::command::notify_subscribers;
#[cfg(feature = "events")]
use crate::subsystems::events::Event;

// guild to use for testing purposes.
#[cfg(debug_assertions)]
const DEBUG_GUILD_ID: &str = env!("LOKI_DEBUG_GUILD_ID");

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

        for s in subsystems() {
            s.ready(&ctx, &ready).await;
        }
    }

    async fn guild_create(&self, ctx: Context, g: Guild, _is_new: bool) {
        let mut data = crate::acquire_data_handle!(write ctx);
        let config = data.get_mut::<Config>().unwrap();
        let guild = config.guild_mut(&g.id);
        let started = guild.threads_started();
        guild.set_threads_started();
        crate::drop_data_handle!(data);
        if !started {
            // start long-running threads for this guild.
            if cfg!(feature = "memes")
                || cfg!(feature = "thread-reviver")
                || cfg!(feature = "nickname-lottery")
                || cfg!(feature = "scoreboard")
            {
                let mut handles: JoinSet<()> = JoinSet::new();
                #[cfg(feature = "memes")]
                handles.spawn(subsystems::memes::MemesVoting::guild_init(
                    ctx.clone(),
                    g.clone(),
                ));
                #[cfg(feature = "thread-reviver")]
                handles.spawn(subsystems::thread_reviver::ThreadReviver::guild_init(
                    ctx.clone(),
                    g.clone(),
                ));
                #[cfg(feature = "nickname-lottery")]
                handles.spawn(subsystems::nickname_lottery::NicknameLottery::guild_init(
                    ctx.clone(),
                    g.clone(),
                ));
                #[cfg(feature = "scoreboard")]
                handles.spawn(subsystems::scoreboard::Scoreboards::guild_init(
                    ctx.clone(),
                    g.clone(),
                ));
                handles.detach_all();
            }
        }
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
                        #[cfg(feature = "events")]
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
        for s in subsystems() {
            s.message(&ctx, &message).await;
        }
    }

    async fn presence_update(&self, ctx: Context, new_data: Presence) {
        info!("Handling Presence updates for {}...", new_data.user.id);
        for s in subsystems() {
            s.presence(&ctx, &new_data).await;
        }
    }

    async fn thread_update(&self, ctx: Context, thread: GuildChannel) {
        for s in subsystems() {
            s.thread(&ctx, &thread).await;
        }
    }

    async fn guild_member_update(&self, ctx: Context, old: Option<Member>, new: Member) {
        for s in subsystems() {
            s.member(&ctx, &old, &new).await;
        }
    }
}

pub fn construct_command(
    cmd: crate::command::Command,
) -> impl FnOnce(&mut CreateApplicationCommand) -> &mut CreateApplicationCommand + '_ {
    move |command: &mut CreateApplicationCommand| {
        let mut command = command
            .name(cmd.name())
            .description(cmd.description())
            .dm_permission(*cmd.permissions() == crate::command::PermissionType::Universal);
        if let crate::command::PermissionType::ServerPerms(permissions) = *cmd.permissions() {
            command = command.default_member_permissions(permissions);
        }
        for variant in cmd.variants() {
            command = command
                .create_option(|subcmd| crate::SerenityHandler::create_variant(subcmd, variant))
        }
        for opt in cmd.options() {
            command = command.create_option(build_opt!(opt))
        }
        command
    }
}

impl<'a> SerenityHandler<'a> {
    /// Construct a new handler from a populated config.
    pub fn new(commands: Vec<crate::command::Command<'a>>) -> Self {
        Self { commands }
    }

    pub(crate) fn create_variant<'b>(
        subcmd: &'b mut CreateApplicationCommandOption,
        variant: &crate::Command,
    ) -> &'b mut CreateApplicationCommandOption {
        let mut subcmd = subcmd
            .name(variant.name())
            .description(variant.description())
            .kind(CommandOptionType::SubCommand)
            .required(false);
        assert!(
            variant.variants().is_empty(),
            "Discord does not currently support nesting CommandGroups."
        );
        // for variant in variant.variants() {
        //     subcmd = subcmd.create_sub_option(|subcmd| Self::create_variant(subcmd, variant));
        // }
        for opt in variant.options() {
            subcmd = subcmd.create_sub_option(build_opt!(opt))
        }
        subcmd
    }

    async fn create_commands(&self, ctx: &Context) -> Vec<Command> {
        macro_rules! commands_constructor {
            () => {
                |mut commands| {
                    for cmd in self.commands.iter().filter(|cmd| cmd.global()) {
                        commands =
                            commands.create_application_command(construct_command(cmd.clone()))
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
                .set_application_commands(&ctx.http, commands_constructor!())
                .await
                .expect("Failed to create command")
        }
        #[cfg(not(debug_assertions))]
        {
            Command::set_global_application_commands(&ctx.http, commands_constructor!())
                .await
                .expect("Failed to create command")
        }
    }
}
