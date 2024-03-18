use std::time::Duration;

use serenity::all::{
    ActionRowComponent, CacheHttp as _, CreateActionRow, CreateModal, Mentionable as _,
};

use crate::config::Config;

use crate::command::{self, Command, PermissionType};
use crate::{create_raw_embed, ActionResponse};

use super::Subsystem;

pub struct StatusMeaning;

impl Subsystem for StatusMeaning {
    fn generate_commands(&self) -> Vec<Command<'static>> {
        vec![
            Command::new(
                "set_status_meaning",
                "Manager-only: sets the meaning of the manager's Discord status.",
                PermissionType::Universal,
                Some(Box::new(move |ctx, command, _params| {
                    Box::pin(async move {
                        let data = crate::acquire_data_handle!(read ctx);
                        let config = data.get::<Config>().unwrap();
                        let manager = config.get_manager().to_user(&ctx.http()).await?;
                        if command.user != manager {
                            let resp =
                                format!("**Unauthorised:** You're not {}!", manager.mention());
                            return Ok(Some(ActionResponse::new(create_raw_embed(resp), true)));
                        }

                        let mut meaning = serenity::builder::CreateInputText::new(
                            serenity::all::InputTextStyle::Paragraph,
                            "Discord status meaning",
                            "new_status_meaning",
                        )
                        .placeholder("Some meaning here, or leave blank to unset.")
                        .required(false);
                        if let Some(old_meaning) = config.get_status_meaning() {
                            meaning = meaning.value(old_meaning);
                        }
                        crate::drop_data_handle!(data);

                        let components = vec![CreateActionRow::InputText(meaning)];

                        command
                            .create_response(
                                &ctx.http(),
                                serenity::all::CreateInteractionResponse::Modal(
                                    CreateModal::new(
                                        "set_status_meaning",
                                        "Set Discord status meaning",
                                    )
                                    .components(components),
                                ),
                            )
                            .await?;

                        // collect the submitted data
                        if let Some(int) = serenity::collector::ModalInteractionCollector::new(ctx)
                            .filter(|int| int.data.custom_id == "set_status_meaning")
                            .timeout(Duration::new(300, 0))
                            .await
                        {
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
                                    if it.custom_id == "new_status_meaning" {
                                        if let Some(it) = &it.value {
                                            if !it.is_empty() {
                                                config.set_status_meaning(Some(it.clone()));
                                            } else {
                                                config.set_status_meaning(None);
                                            }
                                        }
                                    }
                                }
                            }

                            // it's now safe to close the modal, so send a response to it
                            int.create_response(
                                &ctx.http(),
                                serenity::all::CreateInteractionResponse::Acknowledge,
                            )
                            .await?;
                        }

                        Ok(None)
                    })
                })),
            ),
            Command::new(
                "status_meaning",
                "Retrieves the meaning of the bot managers's current Discord status.",
                command::PermissionType::Universal,
                Some(Box::new(move |ctx, _command, _params| {
                    Box::pin(async {
                        let data = crate::acquire_data_handle!(read ctx);
                        let config = data.get::<Config>().unwrap();
                        let manager = config.get_manager().to_user(&ctx.http()).await?.mention();
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
                        Ok(Some(ActionResponse::new(create_raw_embed(&resp), false)))
                    })
                })),
            ),
        ]
    }
}
