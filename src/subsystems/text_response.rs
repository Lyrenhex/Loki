use serenity::async_trait;
use serenity::futures::StreamExt;
use serenity::model::prelude::Message;
use serenity::model::Permissions;
use serenity::prelude::Context;

use crate::config::Config;
use crate::Error;

use crate::command::{
    create_embed, create_response, notify_subscribers, Command, Option, OptionType, PermissionType,
};

use super::Subsystem;

pub struct TextResponse;

#[async_trait]
impl Subsystem for TextResponse {
    fn generate_commands(&self) -> Vec<Command<'static>> {
        vec![
            Command::new(
                "response",
                "Controls for the text response subsystem.",
                PermissionType::Universal,
                None,
            ).add_variant(Command::new(
                "list",
                "List all text inputs which have an associated response set.",
                PermissionType::ServerPerms(Permissions::ADMINISTRATOR),
                Some(Box::new(move |ctx, command| {
                    Box::pin(async move {
                        let data = ctx.data.read().await;
                        if let Some(guild) = crate::config::get_guild(&data, &command.guild_id.unwrap()) {
                            if let Some(response_map) = guild.response_map() {
                                let mut resp = format!("**{} activation phrase(s):**", response_map.keys().count());
                                response_map.keys().for_each(|phrase| resp += format!("\n•\t{phrase}").as_str());
                                drop(data);
                                create_response(&ctx.http, command, &resp, true).await;
                            } else {
                                create_response(&ctx.http, command, &"**No activation phrases.**
Perhaps try adding some?".to_string(), true).await;
                            }
                        } else {
                            return Err(Error::InvalidChannel);
                        }
                        Ok(())
                    })
                })),
            ))
            .add_variant(Command::new(
                "set",
                "Set the response the bot gives to a given text input.",
                PermissionType::ServerPerms(Permissions::ADMINISTRATOR),
                Some(Box::new(move |ctx, command| {
                    Box::pin(async move {
                        let activation_phrase = command.data.options[0]
                            .options
                            .iter()
                            .find(|opt| opt.name == "activation_phrase")
                            .unwrap()
                            .value
                            .as_ref()
                            .unwrap()
                            .as_str()
                            .unwrap();

                        let mut new_response = serenity::builder::CreateInputText::default();
                        new_response
                            .label(format!("Response for \"{}\"", if activation_phrase.len() > 30 {
                                    activation_phrase.chars().take(27).collect::<String>() + "…"
                                } else {
                                    activation_phrase.to_string()
                                }))
                            .custom_id("new_response_value")
                            .style(serenity::model::prelude::component::InputTextStyle::Paragraph)
                            .placeholder("Enter the response to this phrase here, or submit an empty response to unset.")
                            .required(false);
                        let data = ctx.data.read().await;
                        if let Some(guild) = crate::config::get_guild(&data, &command.guild_id.unwrap()) {
                            if let Some(response_map) = guild.response_map() {
                                if let Some(old_response) = response_map.get(activation_phrase) {
                                    new_response.value(old_response);
                                }
                            }
                        }
                        drop(data);

                        let mut components = serenity::builder::CreateComponents::default();
                        components.create_action_row(|r| r.add_input_text(new_response));

                        command
                            .create_interaction_response(&ctx.http, |r| {
                                r.kind(serenity::model::application::interaction::InteractionResponseType::Modal);
                                r.interaction_response_data(|d| {
                                    d.title("Set text response value")
                                        .custom_id("set_response_value")
                                        .set_components(components)
                                })
                            })
                            .await?;

                        let guild_id = command.guild_id.unwrap();

                        // collect the submitted data
                        let collector =
                            serenity::collector::ModalInteractionCollectorBuilder::new(ctx)
                                .filter(|int| int.data.custom_id == "set_response_value")
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

                    for input in inputs.iter() {
                        if let serenity::model::prelude::component::ActionRowComponent::InputText(it) = input {
                            if it.custom_id == "new_response_value" {
                                let guild = config.guild_mut(&guild_id);
                                let response_map = guild.response_map_mut();
                                if !it.value.is_empty() {
                                    response_map.insert(activation_phrase.to_string(), it.value.clone());
                                } else {
                                    response_map.remove(activation_phrase);
                                }
                                config.save();
                            }
                        }
                    }
                    drop(data);

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
                })),
            ).add_option(Option::new(
                "activation_phrase",
                "The phrase which will activate this response when seen.",
                OptionType::StringInput(Some(1), None),
                true,
            ))),
        ]
    }

    async fn message(&self, ctx: &Context, message: &Message) {
        let data = ctx.data.read().await;
        if let Some(guild) = message.guild_id {
            if let Some(guild) = crate::config::get_guild(&data, &guild) {
                if let Some(response_map) = guild.response_map() {
                    for (activator, response) in response_map {
                        if message.content.contains(activator) {
                            if let Ok(channel) = message.channel(&ctx.http).await {
                                if let Some(channel) = channel.guild() {
                                    if let Err(e) = channel
                                        .send_message(&ctx.http, create_embed(response.to_string()))
                                        .await
                                    {
                                        notify_subscribers(
                                            ctx,
                                            super::events::Event::Error,
                                            format!(
                                                "Error in text response handler:
```
{e}
```"
                                            )
                                            .as_str(),
                                        )
                                        .await;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
