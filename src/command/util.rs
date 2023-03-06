use std::sync::Arc;

use log::error;
use serenity::{
    builder::{CreateEmbed, CreateMessage},
    http::Http,
    model::prelude::interaction::{
        application_command::ApplicationCommandInteraction, InteractionResponseType,
    },
    prelude::{Context, HttpError},
    Error,
};

use crate::{config::Config, subsystems::events::Event, COLOUR};

/// Construct a closure for use in [serenity::model::channel::GuildChannel]::send_message
/// from the provided input string.
pub fn create_embed(
    s: String,
) -> impl for<'a, 'b> FnOnce(&'b mut CreateMessage<'a>) -> &'b mut CreateMessage<'a> {
    move |m: &mut CreateMessage| m.add_embed(|e| e.description(s).colour(COLOUR))
}

/// Create a text-based embed response with the given `message`.
pub async fn create_response(
    http: &Arc<Http>,
    interaction: &mut ApplicationCommandInteraction,
    message: &String,
    ephemeral: bool,
) {
    let mut embed = CreateEmbed::default();
    embed.description(message);
    embed.colour(COLOUR);
    match interaction
        .create_interaction_response(&http, |response| {
            response
                .kind(InteractionResponseType::ChannelMessageWithSource)
                .interaction_response_data(|message| {
                    message.add_embed(embed.clone()).ephemeral(ephemeral)
                })
        })
        .await
    {
        Ok(()) => {}
        Err(e) => match e {
            Error::Http(ref e) => match &**e {
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

/// Edit the original text-based embed response, replacing it with
/// the new `embed`.
pub async fn edit_embed_response(
    http: &Arc<Http>,
    interaction: &mut ApplicationCommandInteraction,
    embed: CreateEmbed,
) -> Result<serenity::model::prelude::Message, serenity::Error> {
    interaction
        .edit_original_interaction_response(&http, |message| {
            message
                .content(" ")
                .add_embed(embed)
                .components(|components| components.set_action_rows(vec![]))
        })
        .await
}

/// Notify the subscribers to an event that it has fired.
pub async fn notify_subscribers(ctx: &Context, event: Event, message: &str) {
    let data = ctx.data.read().await;
    let config = data.get::<Config>().unwrap();
    if let Some(subscribers) = config.subscribers(event) {
        for subscriber in subscribers {
            match subscriber.to_user(&ctx.http).await {
                Ok(u) => {
                    if let Err(e) = u
                        .direct_message(
                            &ctx.http,
                            create_embed(format!(
                                "{message}

_You're receiving this message because you're subscribed to the \
`{event}` event._"
                            )),
                        )
                        .await
                    {
                        error!("Could not DM user {subscriber} ({}): {e:?}", u.tag());
                    }
                }
                Err(e) => error!("User {subscriber} could not be resolved: {e:?}"),
            }
        }
    }
}
