mod command;
mod config;
mod error;
mod serenity_handler;
mod subsystems;

pub use log::{error, info};
pub use serenity::{
    prelude::{GatewayIntents, Mentionable},
    utils::Colour,
};

pub use command::{Command, *};
pub use config::{get_guild, Config};
pub use error::Error;
pub use serenity_handler::SerenityHandler;
pub use subsystems::subsystems;

const COLOUR: Colour = Colour::new(0x0099ff);

// retrieve version + repo information from the `Cargo.toml` at
// compile-time.
const VERSION: &str = env!("CARGO_PKG_VERSION");
const GITHUB_URL: &str = env!("CARGO_PKG_REPOSITORY");

macro_rules! acquire_data_handle {
    ($ctx:ident) => { acquire_data_handle!(read $ctx) };
    (read $ctx:ident) => {{
        log::trace!("Acquiring data read handle...");
        let data = $ctx.data.read().await;
        log::trace!("Acquired data read handle.");
        data
    }};
    (write $ctx:ident) => {{
        log::trace!("Acquiring data write handle...");
        let data = $ctx.data.write().await;
        log::trace!("Acquired data write handle.");
        data
    }};
}
macro_rules! drop_data_handle {
    ($data:ident) => {
        drop($data);
        log::trace!("Dropping data handle.");
    };
}
pub(crate) use acquire_data_handle;
pub(crate) use drop_data_handle;

pub type Result = core::result::Result<(), Error>;

/// Construct a string list describing the enabled features.
fn features() -> String {
    let mut features = "".to_string();

    if cfg!(feature = "status-meaning") {
        features += "\n**•** Status meaning information.";
    }
    if cfg!(feature = "memes") {
        features += "\n**•** Meme voting system.";
    }
    if cfg!(feature = "stream-indicator") {
        features += "\n**•** Automatic nickname change when people \
start streaming (excluding server owner).";
    }
    if cfg!(feature = "events") {
        features += "\n**•** Subscriptions to bot events.";
    }
    if cfg!(feature = "thread-reviver") {
        features += "\n**•** Automatic thread revival when they get archived.";
    }
    if cfg!(feature = "text-response") {
        features += "\n**•** Configurable responses to text phrases.";
    }
    if cfg!(feature = "timeout-monitor") {
        features += "\n**•** Timeout monitoring and statistics.";
    }
    if cfg!(feature = "nickname-lottery") {
        features += "\n**•** Randomised, automatic nickname changing.";
    }

    features
}

fn intents() -> GatewayIntents {
    let mut intents = GatewayIntents::non_privileged();

    if cfg!(feature = "guild-presences") {
        intents |= GatewayIntents::GUILD_PRESENCES;
    }
    if cfg!(feature = "guild-members") {
        intents |= GatewayIntents::GUILD_MEMBERS;
    }
    if cfg!(feature = "message-content") {
        intents |= GatewayIntents::MESSAGE_CONTENT;
    }

    intents
}

fn generate_commands() -> Vec<Command<'static>> {
    let mut commands = vec![Command::new(
        "about",
        "Provides information about Loki.",
        command::PermissionType::Universal,
        Some(Box::new(move |ctx, command| {
            Box::pin(async {
                let manager_tag = ctx
                    .data
                    .read()
                    .await
                    .get::<Config>()
                    .unwrap()
                    .get_manager()
                    .to_user(&ctx.http)
                    .await?
                    .mention();
                command::create_response(
                    &ctx.http,
                    command,
                    &format!(
                        "Loki is a trickster ~~god~~ bot.
Version [{VERSION}]({GITHUB_URL}/releases/tag/v{VERSION}); [source code]({GITHUB_URL}).

This instance of Loki is managed by {manager_tag}.

Currently enabled features: {}",
                        features()
                    ),
                    false,
                )
                .await;
                Ok(())
            })
        })),
    )];
    subsystems()
        .iter()
        .for_each(|s| commands.append(&mut s.generate_commands()));

    commands
}

pub async fn run() {
    env_logger::init();

    info!("Starting up...");

    let config = Config::load();

    let commands = generate_commands();

    let handler = SerenityHandler::new(commands);

    // Login with a bot token from the environment
    let mut client = config
        .discord_client(intents())
        .event_handler(handler)
        .await
        .expect("Error creating client");

    {
        let mut data = client.data.write().await;
        data.insert::<Config>(config);
    }

    loop {
        // start listening for events by starting a single shard
        if let Err(err) = client.start().await {
            // unknown error (fatal): announce and terminate.
            error!("*FATAL*: {:?}", err);
            break;
        }
    }
}
