use serenity::futures::StreamExt;
use serenity::prelude::Mentionable;

use crate::config::Config;

use crate::command::{self, create_response, Command, PermissionType};

use super::Subsystem;

pub struct StatusMeaning;

impl Subsystem for StatusMeaning {
    fn generate_commands(&self) -> Vec<Command<'static>> {
        vec![
            Command::new(
                "set_status_meaning",
                "Manager-only: sets the meaning of the manager's Discord status.",
                PermissionType::Universal,
                Some(Box::new(move |ctx, command| {
                    Box::pin(async move {
                        let data = ctx.data.read().await;
                        let config = data.get::<Config>().unwrap();
                        let manager = config.get_manager().to_user(&ctx.http).await?;
                        if command.user != manager {
                            let resp =
                                format!("**Unauthorised:** You're not {}!", manager.mention());
                            create_response(&ctx.http, command, &resp, true).await;
                            return Ok(());
                        }

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
                                r.kind(
                                    serenity::model::application::interaction::InteractionResponseType::Modal,
                                );
                                r.interaction_response_data(|d| {
                                    d.title("Set Discord status meaning")
                                        .custom_id("set_status_meaning")
                                        .set_components(components)
                                })
                            })
                            .await?;

                        // collect the submitted data
                        let collector =
                            serenity::collector::ModalInteractionCollectorBuilder::new(ctx)
                                .filter(|int| int.data.custom_id == "set_status_meaning")
                                .collect_limit(1)
                                .build();

                        collector.then(|int| async move {
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
                                    if it.custom_id == "new_status_meaning" {
                                        if !it.value.is_empty() {
                                            config.set_status_meaning(Some(it.value.clone()));
                                        } else {
                                            config.set_status_meaning(None);
                                        }
                                    }
                                }
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
                        let manager = config.get_manager().to_user(&ctx.http).await?.mention();
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
                        create_response(&ctx.http, command, &resp, false).await;
                        Ok(())
                    })
                })),
            ),
        ]
    }
}
