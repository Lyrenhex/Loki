mod command;
mod config;
mod error;
mod serenity_handler;

use log::error;
use serenity::{futures::StreamExt, prelude::GatewayIntents};

use command::Command;
use config::Config;
pub use error::Error;
use serenity_handler::SerenityHandler;

// retrieve version + repo information from the `Cargo.toml` at
// compile-time.
const VERSION: &str = env!("CARGO_PKG_VERSION");
const GITHUB_URL: &str = env!("CARGO_PKG_REPOSITORY");
const FEATURES: &str =
    "- `/status_meaning` to determine the meaning of the bot manager's Discord status.";

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
                        .await?
                        .tag();
                    command::create_response(
                        &ctx.http,
                        command,
                        &format!(
                            "Loki is a trickster ~~god~~ bot.
Version {VERSION}; [source code]({GITHUB_URL}).

This instance of Loki is managed by {manager_tag}.

Current features:
{FEATURES}"
                        ),
                    )
                    .await;
                    Ok(())
                })
            }),
        ),
        Command::new(
            "status_meaning",
            "Retrieves the meaning of the bot managers's current Discord status.",
            command::PermissionType::Universal,
            Box::new(move |ctx, command| {
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
            }),
        ),
        Command::new(
            "set_status_meaning",
            "Manager-only: sets the meaning of the manager's Discord status.",
            command::PermissionType::Universal,
            Box::new(move |ctx, command| {
                Box::pin(async move {
                    let data = ctx.data.read().await;
                    let config = data.get::<Config>().unwrap();
                    let manager = config.get_manager().to_user(&ctx.http).await?;
                    if command.user != manager {
                        let resp = format!("**Unauthorised:** You're not {}!", manager.tag());
                        command::create_response(&ctx.http, command, &resp).await;
                        return Ok(());
                    }

                    // TODO: refactor everything after this - maybe a
                    // custom struct which handles all of this for us?
                    // eg, sets IDs and handles returning values etc.
                    // then, we can just call it, await a response,
                    // and set our values...
                    // for now though, credit to https://github.com/aquelemiguel/parrot/blob/main/src/commands/manage_sources.rs
                    // for this design.
                    let mut meaning = serenity::builder::CreateInputText::default();
                    meaning
                        .label("Discord status meaning")
                        .custom_id("new_status_meaning")
                        .style(serenity::model::prelude::component::InputTextStyle::Paragraph)
                        .placeholder("Some meaning here, or leave blank to unset.")
                        .required(false);
                    if let Some(old_meaning) = config.get_status_meaning() {
                        meaning.value(old_meaning);
                    }
                    drop(data);

                    let mut components = serenity::builder::CreateComponents::default();
                    components.create_action_row(|r| r.add_input_text(meaning));

                    command
                        .create_interaction_response(&ctx.http, |r| {
                            r.kind(serenity::model::application::interaction::InteractionResponseType::Modal);
                            r.interaction_response_data(|d| {
                                d.title("Set Discord status meaning").custom_id("set_status_meaning").set_components(components)
                            })
                        })
                        .await?;

                    // collect the submitted data
                    let collector = serenity::collector::ModalInteractionCollectorBuilder::new(ctx)
                        .filter(|int| int.data.custom_id == "set_status_meaning")
                        .build();

                    collector
                        .then(|int| async move {
                            let mut data = ctx.data.write().await;
                            let config = data.get_mut::<Config>().unwrap();

                            let inputs: Vec<_> = int
                                .data
                                .components
                                .iter()
                                .flat_map(|r| r.components.iter())
                                .collect();

                            let mut updated = false;

                            for input in inputs.iter() {
                                if let serenity::model::prelude::component::ActionRowComponent::InputText(it) = input {
                                    if it.custom_id == "new_status_meaning" && it.value != "" {
                                        config.set_status_meaning(Some(it.value.clone()));
                                        updated = true;
                                    }
                                }
                            }

                            if !updated {
                                config.set_status_meaning(None);
                            }

                            // it's now safe to close the modal, so send a response to it
                            int.create_interaction_response(&ctx.http, |r| {
                                r.kind(serenity::model::prelude::interaction::InteractionResponseType::DeferredUpdateMessage)
                            })
                            .await
                            .ok();
                        })
                        .collect::<Vec<_>>()
                        .await;

                    Ok(())
                })
            }),
        ),
    ]
}
