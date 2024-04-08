use std::sync::Arc;

use log::error;
use serenity::{
    all::{CreateInteractionResponseMessage, EditInteractionResponse},
    builder::{CreateEmbed, CreateMessage},
    http::Http,
    model::application::CommandInteraction,
    prelude::HttpError,
    Error,
};

use crate::COLOUR;

#[cfg(feature = "events")]
use crate::{config::Config, subsystems::events::Event};
#[cfg(feature = "events")]
use serenity::prelude::{Context, TypeMap};
#[cfg(feature = "events")]
use tokio::sync::RwLockReadGuard;

/// Construct a closure for use in [serenity::model::channel::GuildChannel]::send_message
/// from the provided input string.
pub fn create_embed(s: String) -> CreateMessage {
    CreateMessage::new().add_embed(CreateEmbed::default().description(s).colour(COLOUR))
}

/// Construct a closure for use in [serenity::model::channel::GuildChannel]::send_message
/// from the provided input string.
pub fn create_raw_embed(s: impl Into<String>) -> CreateEmbed {
    CreateEmbed::default().description(s).colour(COLOUR)
}

/// Create an embed response.
pub async fn create_response_from_embed(
    http: &Arc<Http>,
    interaction: &mut CommandInteraction,
    embed: CreateEmbed,
    ephemeral: bool,
) {
    match interaction
        .create_response(
            &http,
            serenity::all::CreateInteractionResponse::Message(
                CreateInteractionResponseMessage::new()
                    .add_embed(embed.clone())
                    .ephemeral(ephemeral),
            ),
        )
        .await
    {
        Ok(()) => {}
        Err(e) => match e {
            Error::Http(ref e) => match &e {
                HttpError::UnsuccessfulRequest(req) => match req.error.code {
                    40060 => {
                        edit_embed_response(http, interaction, embed).await.unwrap();
                    }
                    _ => error!("{}", e),
                },
                _ => error!("{}", e),
            },
            _ => error!("{}", e),
        },
    }
}

/// Create a text-based embed response with the given `message`.
pub async fn create_response(
    http: &Arc<Http>,
    interaction: &mut CommandInteraction,
    message: &String,
    ephemeral: bool,
) {
    let embed = create_raw_embed(message);
    create_response_from_embed(http, interaction, embed, ephemeral).await
}

/// Edit the original text-based embed response, replacing it with
/// the new `embed`.
pub async fn edit_embed_response(
    http: &Arc<Http>,
    interaction: &mut CommandInteraction,
    embed: CreateEmbed,
) -> Result<serenity::model::prelude::Message, serenity::Error> {
    interaction
        .edit_response(
            &http,
            EditInteractionResponse::new()
                .content(" ")
                .add_embed(embed)
                .components(Vec::new()),
        )
        .await
}

/// Notify the subscribers to an event that it has fired.
#[cfg(feature = "events")]
pub async fn notify_subscribers(ctx: &Context, event: Event, message: &str) {
    use serenity::all::CacheHttp as _;

    let data = crate::acquire_data_handle!(read ctx);
    let config = data.get::<Config>().unwrap();
    if let Some(subscribers) = config.subscribers(event) {
        for subscriber in subscribers {
            match subscriber.to_user(&ctx.http()).await {
                Ok(u) => {
                    if let Err(e) = u
                        .direct_message(
                            &ctx.http(),
                            create_embed(format!(
                                "{message}

_You're receiving this message because you're subscribed to the \
`{event}` event._"
                            )),
                        )
                        .await
                    {
                        error!("Could not DM user {subscriber} ({}): {e:?}", u.name);
                    }
                }
                Err(e) => error!("User {subscriber} could not be resolved: {e:?}"),
            }
        }
    }
}

/// Notify the subscribers to an event that it has fired, using an existing
/// read handle for global data.
#[cfg(feature = "events")]
pub async fn notify_subscribers_with_handle(
    ctx: &Context,
    data: &RwLockReadGuard<'_, TypeMap>,
    event: Event,
    message: &str,
) {
    use serenity::all::CacheHttp as _;

    let config = data.get::<Config>().unwrap();
    if let Some(subscribers) = config.subscribers(event) {
        for subscriber in subscribers {
            match subscriber.to_user(&ctx.http()).await {
                Ok(u) => {
                    if let Err(e) = u
                        .direct_message(
                            &ctx.http(),
                            create_embed(format!(
                                "{message}

_You're receiving this message because you're subscribed to the \
`{event}` event._"
                            )),
                        )
                        .await
                    {
                        error!("Could not DM user {subscriber} ({}): {e:?}", u.name);
                    }
                }
                Err(e) => error!("User {subscriber} could not be resolved: {e:?}"),
            }
        }
    }
}
