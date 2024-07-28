use crate::command::OptionType;
use crate::config::Config;
use crate::subsystems;
use log::{error, info, trace, warn};
use serenity::all::{
    ActivityData, CacheHttp as _, Command, CommandDataOptionValue, CommandOptionType,
    GuildMemberUpdateEvent, Interaction,
};
use serenity::builder::{CreateCommand, CreateCommandOption};
#[cfg(debug_assertions)]
use serenity::model::prelude::GuildId;
use serenity::model::prelude::{GuildChannel, Member};
use serenity::{
    async_trait,
    model::prelude::{Guild, Message, Presence, Ready},
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

/// Core implementation logic for [serenity] events.
pub struct SerenityHandler<'a> {
    commands: Vec<crate::command::Command<'a>>,
}

#[async_trait]
impl EventHandler for SerenityHandler<'_> {
    async fn ready(&self, ctx: Context, ready: Ready) {
        info!("Loki is connected as {}", ready.user.name);

        ctx.set_activity(Some(ActivityData::playing("tricks")));

        // creates the global application commands
        self.create_commands(&ctx).await;

        for s in subsystems() {
            s.ready(&ctx, &ready).await;
        }
    }

    async fn guild_create(&self, ctx: Context, g: Guild, is_new: Option<bool>) {
        info!("Guild Creation event for {} (new: {is_new:?})", g.id);
        trace!("Guild Creation data: {g:?}");
        let mut data = crate::acquire_data_handle!(write ctx);
        let config = data.get_mut::<Config>().unwrap();
        let guild = config.guild_mut(&g.id);
        let started = guild.threads_started();
        guild.set_threads_started();
        crate::drop_data_handle!(data);
        if !started {
            info!(
                "Starting background threads for guild {} ({}).",
                g.id, g.name
            );
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
        trace!("Handling Interaction: {:?}", interaction);
        if let Interaction::Command(mut command) = interaction {
            for cmd in self.commands.iter() {
                if cmd.name() == command.data.name {
                    let mut cmd = cmd;
                    let mut options = command.data.options.clone();
                    if !command.data.options.is_empty()
                        && matches!(
                            command.data.options[0].kind(),
                            CommandOptionType::SubCommand | CommandOptionType::SubCommandGroup
                        )
                    {
                        // TODO: This is a little... unpleasant.
                        // At some point it'd be good to refactor this to be recursive, like how we generate these group structures in the first place.
                        for subcmd in cmd.variants() {
                            if subcmd.name() == command.data.options[0].name {
                                cmd = subcmd;
                                if let CommandDataOptionValue::SubCommandGroup(os) =
                                    &command.data.options[0].value
                                {
                                    options.clone_from(os);
                                    for subcmd in cmd.variants() {
                                        if subcmd.name() == os[0].name {
                                            cmd = subcmd;
                                            if let CommandDataOptionValue::SubCommand(os) =
                                                &os[0].value
                                            {
                                                options.clone_from(os);
                                            } else {
                                                error!("Failed to extract subcommand options from {command:?}");
                                            }
                                            break;
                                        }
                                    }
                                } else if let CommandDataOptionValue::SubCommand(os) =
                                    &command.data.options[0].value
                                {
                                    options.clone_from(os);
                                } else {
                                    error!("Failed to extract subcommand options from {command:?}");
                                }
                                break;
                            }
                        }
                    };
                    match cmd.run(&ctx, &mut command, &options).await {
                        Ok(e) => {
                            if let Some(e) = e {
                                let ephemeral = e.ephemeral();
                                crate::command::create_response_from_embed(
                                    &ctx.http,
                                    &mut command,
                                    e.embed(),
                                    ephemeral,
                                )
                                .await;
                            }
                        }
                        Err(e) => {
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
                    }
                    break;
                }
            }
        };
    }

    async fn message(&self, ctx: Context, message: Message) {
        trace!("Handling Message: {:?}", message);
        for s in subsystems() {
            s.message(&ctx, &message).await;
        }
    }

    async fn presence_update(&self, ctx: Context, new_data: Presence) {
        trace!("Handling Presence update: {:?}", new_data);
        for s in subsystems() {
            s.presence(&ctx, &new_data).await;
        }
    }

    async fn thread_update(&self, ctx: Context, _old: Option<GuildChannel>, thread: GuildChannel) {
        trace!("Handling Thread update: {:?}", thread);
        for s in subsystems() {
            s.thread(&ctx, &thread).await;
        }
    }

    async fn guild_member_update(
        &self,
        ctx: Context,
        old: Option<Member>,
        new: Option<Member>,
        event: GuildMemberUpdateEvent,
    ) {
        trace!("Handling Guild Member update: {:?} --> {:?}", old, new);
        if let Some(new) = new {
            for s in subsystems() {
                s.member(&ctx, &old, &new).await;
            }
        } else {
            warn!("No new data when handling Guild Member update: {old:?} --> {new:?} ({event:?})");
        }
    }
}

pub fn construct_command(cmd: &crate::command::Command) -> CreateCommand {
    let mut command = CreateCommand::new(cmd.name())
        .description(cmd.description())
        .dm_permission(*cmd.permissions() == crate::command::PermissionType::Universal);
    if let crate::command::PermissionType::ServerPerms(permissions) = *cmd.permissions() {
        command = command.default_member_permissions(permissions);
    }
    for variant in cmd.variants() {
        command = command.add_option(crate::SerenityHandler::create_variant(variant, true))
    }
    for opt in cmd.options() {
        command = command.add_option(construct_option(opt))
    }
    command
}

pub fn construct_option(opt: &crate::command::Option) -> CreateCommandOption {
    let mut option = CreateCommandOption::new(opt.kind().into(), opt.name(), opt.description())
        .required(opt.required());
    match opt.kind() {
        OptionType::StringInput(min, max) => {
            if let Some(min) = min {
                option = option.clone().min_length(min);
            }
            if let Some(max) = max {
                option = option.clone().max_length(max);
            }
        }
        OptionType::StringSelect(options) => {
            options.iter().for_each(|s| {
                option = option.clone().add_string_choice(s, s);
            });
        }
        OptionType::IntegerInput(min, max) => {
            // Note: The `try_into.unwrap` portion is included to work around a bug introduced by Serenity 0.12; once that bug is fixed (which will in turn make these lines break!), remove these.
            // Issue: https://github.com/serenity-rs/serenity/issues/2652
            // Fixed in PR: https://github.com/serenity-rs/serenity/pull/2668
            // TODO: When this lands in a release version, adjust these back.
            if let Some(min) = min {
                option = option
                    .clone()
                    .min_int_value(min.try_into().unwrap_or(u64::MIN));
            }
            if let Some(max) = max {
                option = option
                    .clone()
                    .max_int_value(max.try_into().unwrap_or(u64::MAX));
            }
        }
        OptionType::IntegerSelect(options) => {
            options.iter().for_each(|s| {
                option = option.clone().add_int_choice(s.to_string(), *s as i32);
            });
        }
        OptionType::NumberInput(min, max) => {
            if let Some(min) = min {
                option = option.clone().min_number_value(min);
            }
            if let Some(max) = max {
                option = option.clone().max_number_value(max);
            }
        }
        OptionType::NumberSelect(options) => {
            options.iter().for_each(|s| {
                option = option.clone().add_number_choice(s.to_string(), *s);
            });
        }
        OptionType::Channel(types) => {
            if let Some(types) = types {
                option = option.clone().channel_types(types);
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

impl<'a> SerenityHandler<'a> {
    /// Construct a new handler from a populated config.
    pub fn new(commands: Vec<crate::command::Command<'a>>) -> Self {
        Self { commands }
    }

    pub(crate) fn create_variant(
        variant: &crate::Command,
        allow_subcommands: bool,
    ) -> CreateCommandOption {
        let mut subcmd = if allow_subcommands {
            if variant.variants().is_empty() {
                CreateCommandOption::new(
                    CommandOptionType::SubCommand,
                    variant.name(),
                    variant.description(),
                )
                .required(false)
            } else {
                let mut subcmd = CreateCommandOption::new(
                    CommandOptionType::SubCommandGroup,
                    variant.name(),
                    variant.description(),
                )
                .required(false);
                for variant in variant.variants() {
                    subcmd = subcmd.add_sub_option(Self::create_variant(variant, false));
                }
                subcmd
            }
        } else {
            assert!(
                variant.variants().is_empty(),
                "Discord currently allows a top-level command to contain subcommand groups, which each contain direct subcommands. No further nesting is supported."
            );
            CreateCommandOption::new(
                CommandOptionType::SubCommand,
                variant.name(),
                variant.description(),
            )
            .required(false)
        };
        for opt in variant.options() {
            subcmd = subcmd.add_sub_option(construct_option(opt))
        }
        subcmd
    }

    async fn create_commands(&self, ctx: &Context) -> Vec<Command> {
        let commands = self
            .commands
            .iter()
            .filter(|cmd| cmd.global())
            .map(construct_command)
            .collect::<Vec<CreateCommand>>();
        #[cfg(debug_assertions)]
        {
            let guild = GuildId::new(DEBUG_GUILD_ID.parse::<u64>().unwrap_or_else(|_| {
                panic!(
                    "{}",
                    ("Couldn't parse 'LOKI_DEBUG_GUILD_ID' as a u64: ".to_owned() + DEBUG_GUILD_ID)
                )
            }))
            .to_partial_guild(&ctx.http())
            .await
            .unwrap();
            guild
                .set_commands(&ctx.http, commands)
                .await
                .expect("Failed to create command")
        }
        #[cfg(not(debug_assertions))]
        {
            Command::set_global_commands(&ctx.http(), commands)
                .await
                .expect("Failed to create command")
        }
    }
}
