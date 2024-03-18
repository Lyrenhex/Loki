use std::time::Duration;

use serenity::all::{ActionRowComponent, CacheHttp as _, CreateActionRow, CreateModal};
use serenity::async_trait;
use serenity::model::prelude::Message;
use serenity::model::Permissions;
use serenity::prelude::Context;

use crate::config::Config;
use crate::{create_raw_embed, ActionResponse, Error};

use crate::command::{
    create_embed, notify_subscribers, Command, Option, OptionType, PermissionType,
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
                Some(Box::new(move |ctx, command, _params| {
                    Box::pin(async move {
                        let data = crate::acquire_data_handle!(read ctx);
                        if let Some(guild) = crate::config::get_guild(&data, &command.guild_id.unwrap()) {
                            if let Some(response_map) = guild.response_map() {
                                let mut resp = format!("**{} activation phrase(s):**", response_map.keys().count());
                                response_map.keys().for_each(|phrase| resp += format!("\n•\t{phrase}").as_str());
                                crate::drop_data_handle!(data);
                                Ok(Some(ActionResponse::new(create_raw_embed(&resp), true)))
                            } else {
                                Ok(Some(ActionResponse::new(create_raw_embed("**No activation phrases.**
Perhaps try adding some?"), true)))
                            }
                        } else {
                            Err(Error::InvalidChannel)
                        }
                    })
                })),
            ))
            .add_variant(Command::new(
                "set",
                "Set the response the bot gives to a given text input.",
                PermissionType::ServerPerms(Permissions::ADMINISTRATOR),
                Some(Box::new(move |ctx, command, params| {
                    Box::pin(async move {
                        let activation_phrase = get_param!(params, String, "activation_phrase");

                        let mut new_response = serenity::builder::CreateInputText::new(serenity::all::InputTextStyle::Paragraph, format!("Response for \"{}\"", if activation_phrase.len() > 30 {
                                    activation_phrase.chars().take(27).collect::<String>() + "…"
                                } else {
                                    activation_phrase.to_string()
                                }), "new_response_value").placeholder("Enter the response to this phrase here, or submit an empty response to unset.")
                            .required(false);
                        let data = crate::acquire_data_handle!(read ctx);
                        if let Some(guild) = crate::config::get_guild(&data, &command.guild_id.unwrap()) {
                            if let Some(response_map) = guild.response_map() {
                                if let Some(old_response) = response_map.get(activation_phrase) {
                                    new_response = new_response.value(old_response);
                                }
                            }
                        }
                        crate::drop_data_handle!(data);

                        let components = vec![CreateActionRow::InputText(new_response)];

                        command
                            .create_response(&ctx.http(), serenity::all::CreateInteractionResponse::Modal(CreateModal::new("set_response_value", "Set text response value").components(components)))
                            .await?;

                        let guild_id = command.guild_id.unwrap();

                        // collect the submitted data
                        if let Some(int) =
                            serenity::collector::ModalInteractionCollector::new(ctx)
                                .filter(|int| int.data.custom_id == "set_response_value")
                                .timeout(Duration::new(300, 0)).await {
                            let mut data = crate::acquire_data_handle!(write ctx);
                            let config = data.get_mut::<Config>().unwrap();

                            let inputs: Vec<_> = int
                                .data
                                .components
                                .iter()
                                .flat_map(|r| r.components.iter())
                                .collect();

                            for input in inputs.iter() {
                                if let ActionRowComponent::InputText(it) = input {
                                    if it.custom_id == "new_response_value" {
                                        let guild = config.guild_mut(&guild_id);
                                        let response_map = guild.response_map_mut();
                                        if let Some(it) = &it.value {
                                            if !it.is_empty() {
                                            response_map.insert(activation_phrase.to_string().to_lowercase(), it.clone());
                                        } else {
                                            response_map.remove(&activation_phrase.to_lowercase());
                                        }
                                        config.save();
                                        }
                                    }
                                }
                            }
                            crate::drop_data_handle!(data);

                            // it's now safe to close the modal, so send a response to it
                            int.create_response(&ctx.http(), serenity::all::CreateInteractionResponse::Acknowledge)
                            .await?;
                        }

                        Ok(None)
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
        let data = crate::acquire_data_handle!(read ctx);
        if let Some(guild) = message.guild_id {
            if let Some(guild) = crate::config::get_guild(&data, &guild) {
                if let Some(response_map) = guild.response_map() {
                    for (activator, response) in response_map {
                        if message.content.to_lowercase().contains(activator) {
                            if let Ok(channel) = message.channel(&ctx.http()).await {
                                if let Some(channel) = channel.guild() {
                                    if let Err(e) = channel
                                        .send_message(
                                            &ctx.http(),
                                            create_embed(response.to_string()),
                                        )
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
