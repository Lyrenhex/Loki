mod command;
mod config;
mod error;
mod serenity_handler;
mod subsystems;

use log::error;
use serenity::{
    prelude::{GatewayIntents, Mentionable},
    utils::Colour,
};

use command::Command;
use config::Config;
pub use error::Error;
use serenity_handler::SerenityHandler;
pub use subsystems::subsystems;

const COLOUR: Colour = Colour::new(0x0099ff);
const DATE_FMT: &str = "%l:%M%P on %A %e %B %Y";

// retrieve version + repo information from the `Cargo.toml` at
// compile-time.
const VERSION: &str = env!("CARGO_PKG_VERSION");
const GITHUB_URL: &str = env!("CARGO_PKG_REPOSITORY");
const FEATURES: &str =
    "- `/status_meaning` to determine the meaning of the bot manager's Discord status.
- Meme voting system.
- Automatic nickname change when people start streaming. \
(Note that this is not available for the server owner...)
- Subscriptions to bot events.";

pub type Result = core::result::Result<(), Error>;

#[tokio::main]
async fn main() {
    env_logger::init();

    let config = Config::load();

    let commands = generate_commands();

    let handler = SerenityHandler::new(commands);

    // Login with a bot token from the environment
    let mut client = config
        .discord_client(GatewayIntents::non_privileged() | GatewayIntents::GUILD_PRESENCES)
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

Current features:
{FEATURES}"
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
