use log::error;
use std::{pin::Pin, sync::Arc};

use serenity::{
    builder::CreateEmbed,
    http::Http,
    model::{
        prelude::interaction::{
            application_command::ApplicationCommandInteraction, InteractionResponseType,
        },
        Permissions,
    },
    prelude::{Context, HttpError},
    Error,
};

type ActionRoutine = Box<
    dyn (for<'b> Fn(
            &'b Context,
            &'b mut ApplicationCommandInteraction,
        )
            -> Pin<Box<dyn std::future::Future<Output = crate::Result> + Send + Sync + 'b>>)
        + Sync
        + Send,
>;

#[derive(Debug, PartialEq)]
pub enum PermissionType {
    /// Available for use by anyone (including in DMs).
    /// Note that individual commands may, in certain circumstances,
    /// check manually for specific criteria being met.
    Universal,
    /// Command requires specific permissions within the server.
    /// This disables the command in DMs.
    ServerPerms(Permissions),
}

/// Meta-information about a command.
///
/// A vector of these objects is used to create the Discord-side
/// "slash commands", and this vector is then used by comparing to
/// a triggered slash command to determine which routine to execute.
pub struct Command<'a> {
    name: &'a str,
    description: &'a str,
    permissions: PermissionType,
    action: ActionRoutine,
}

impl<'a> Command<'a> {
    /// Construct a new Command with the given name and description,
    /// which performs the given [ActionRoutine] when called.
    ///
    /// ## Example
    ///
    /// ```
    /// Command::new(
    ///     "name",
    ///     "A description of what the command does.",
    ///     Box::new(move |ctx, command| {
    ///         Box::pin(async {
    ///             // do something here
    ///         })
    ///     }),
    /// ),
    /// ```
    pub fn new(
        name: &'a str,
        description: &'a str,
        permissions: PermissionType,
        action: ActionRoutine,
    ) -> Self {
        if description.len() > 100 {
            panic!("Description should be <= 100 characters. (Command: {name})");
        }
        Self {
            name,
            description,
            permissions,
            action,
        }
    }

    /// Get the [Command]'s name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Get the [Command]'s description.
    pub fn description(&self) -> &str {
        &self.description
    }

    /// Get the [PermissionType] for the [Command].
    pub fn permissions(&self) -> &PermissionType {
        &self.permissions
    }

    /// Run the [ActionRoutine] for this [Command].
    pub async fn run(
        &self,
        ctx: &Context,
        command: &mut ApplicationCommandInteraction,
    ) -> crate::Result {
        (self.action)(ctx, command).await
    }
}

pub async fn create_response(
    http: &Arc<Http>,
    interaction: &mut ApplicationCommandInteraction,
    message: &String,
) {
    let mut embed = CreateEmbed::default();
    embed.description(message);
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

async fn edit_embed_response(
    http: &Arc<Http>,
    interaction: &mut ApplicationCommandInteraction,
    embed: CreateEmbed,
) -> Result<serenity::model::prelude::Message, serenity::Error> {
    interaction
        .edit_original_interaction_response(&http, |message| message.content(" ").add_embed(embed))
        .await
}
