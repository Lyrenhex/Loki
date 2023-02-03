use std::sync::Arc;

use log::error;
use serenity::{
    builder::CreateEmbed,
    http::Http,
    model::prelude::interaction::{
        application_command::ApplicationCommandInteraction, InteractionResponseType,
    },
    prelude::HttpError,
    Error,
};

use crate::COLOUR;

/// Create a text-based embed response with the given `message`.
pub async fn create_response(
    http: &Arc<Http>,
    interaction: &mut ApplicationCommandInteraction,
    message: &String,
) {
    let mut embed = CreateEmbed::default();
    embed.description(message);
    embed.colour(COLOUR);
    match interaction
        .create_interaction_response(&http, |response| {
            response
                .kind(InteractionResponseType::ChannelMessageWithSource)
                .interaction_response_data(|message| message.add_embed(embed.clone()))
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
