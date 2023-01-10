mod command;
mod config;
mod serenity_handler;

use log::error;
use serenity::prelude::GatewayIntents;

use command::Command;
use config::Config;
use serenity_handler::SerenityHandler;

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
            Box::new(move |ctx, command| {
                Box::pin(async {
                    let manager_tag = ctx
                        .data
                        .read()
                        .await
                        .get::<Config>()
                        .unwrap()
                        .get_manager()
                        .to_user(&ctx.http)
                        .await
                        .unwrap()
                        .tag();
                    command::create_response(
                        &ctx.http,
                        command,
                        &format!(
                            "Loki is a trickster ~~god~~ bot.
This is a rolling release.
You can [find the source here](https://github.com/Lyrenhex/Loki).

This instance of Loki is managed by {manager_tag}."
                        ),
                    )
                    .await;
                })
            }),
        ),
        Command::new(
            "status_meaning",
            "Retrieves the meaning of the bot managers's current Discord status.",
            Box::new(move |ctx, command| {
                Box::pin(async {
                    let data = ctx.data.read().await;
                    let config = data.get::<Config>().unwrap();
                    let manager = config.get_manager().to_user(&ctx.http).await.unwrap().tag();
                    let resp = match config.get_status_meaning() {
                        Some(meaning) => format!(
                            "**Status meaning:**

> {meaning}

_If this meaning doesn't make sense, yell at {manager} to update \
this!_"
                        ),
                        None => format!(
                            "**No known meaning.**

Assuming there _is_, in fact, a status message, you likely need to \
prod {manager} to update this."
                        ),
                    };
                    command::create_response(&ctx.http, command, &resp).await;
                })
            }),
        ),
    ]
}
