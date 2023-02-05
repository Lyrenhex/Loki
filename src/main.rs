mod command;
mod config;
mod error;
mod serenity_handler;

// use std::time::Duration;

use log::error;
use serenity::{prelude::GatewayIntents, utils::Colour};

use command::Command;
use config::Config;
pub use error::Error;
use serenity_handler::SerenityHandler;

const COLOUR: Colour = Colour::new(0x0099ff);
const DATE_FMT: &str = "%k:%M%P on %A %e %B %Y";

// retrieve version + repo information from the `Cargo.toml` at
// compile-time.
const VERSION: &str = env!("CARGO_PKG_VERSION");
const GITHUB_URL: &str = env!("CARGO_PKG_REPOSITORY");
const FEATURES: &str =
    "- `/status_meaning` to determine the meaning of the bot manager's Discord status.
    - `/memes` (`Manage Channels` permissions required) to control the meme voting system.";

pub type Result = core::result::Result<(), Error>;

#[tokio::main]
async fn main() {
    env_logger::init();

    let config = Config::load();

    let commands = generate_commands();

    let handler = SerenityHandler::new(commands);

    // Login with a bot token from the environment
    let mut client = config
        .discord_client(GatewayIntents::non_privileged())
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
            match err {
                _ => {
                    // unknown error (fatal): announce and terminate.
                    error!("*FATAL*: {:?}", err);
                    break;
                }
            }
        }
    }
}

fn generate_commands() -> Vec<Command<'static>> {
    vec![
        Command::new(
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
                        .tag();
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
                    )
                    .await;
                    Ok(())
                })
            })),
        ),
        Command::new(
            "status_meaning",
            "Retrieves the meaning of the bot managers's current Discord status.",
            command::PermissionType::Universal,
            Some(Box::new(move |ctx, command| {
                Box::pin(async {
                    let data = ctx.data.read().await;
                    let config = data.get::<Config>().unwrap();
                    let manager = config.get_manager().to_user(&ctx.http).await?.tag();
                    let resp = match config.get_status_meaning() {
                        Some(meaning) => format!(
                            "**Status meaning:**
{meaning}

_If this meaning seems out-of-date, yell at {manager} to update \
this!_"
                        ),
                        None => format!(
                            "**No known meaning.**

Assuming there _is_, in fact, a status message, you likely need to \
prod {manager} to update this."
                        ),
                    };
                    command::create_response(&ctx.http, command, &resp).await;
                    Ok(())
                })
            })),
        ),
        command::set_status_meaning(),
        command::memes_channel_mgmt(),
    ]
}
