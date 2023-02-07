mod memes_channel_mgmt;
mod set_status_meaning;
mod util;

pub use memes_channel_mgmt::memes_channel_mgmt;
pub use set_status_meaning::set_status_meaning;
pub use util::*;

use std::pin::Pin;

use serenity::{
    model::{
        prelude::{
            command::CommandOptionType,
            interaction::application_command::ApplicationCommandInteraction,
        },
        Permissions,
    },
    prelude::Context,
};

use crate::Error;

type ActionRoutine = Box<
    dyn (for<'b> Fn(
            &'b Context,
            &'b mut ApplicationCommandInteraction,
        )
            -> Pin<Box<dyn std::future::Future<Output = crate::Result> + Send + Sync + 'b>>)
        + Sync
        + Send,
>;

#[derive(Debug, PartialEq, Eq)]
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
    options: Vec<Option<'a>>,
    variants: Vec<Command<'a>>,
    action: std::option::Option<ActionRoutine>,
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
        action: std::option::Option<ActionRoutine>,
    ) -> Self {
        if description.len() > 100 {
            panic!("Description should be <= 100 characters. (Command: {name})");
        }
        Self {
            name,
            description,
            permissions,
            options: Vec::new(),
            variants: Vec::new(),
            action,
        }
    }

    /// Get the [Command]'s name.
    pub fn name(&self) -> &str {
        self.name
    }

    /// Get the [Command]'s description.
    pub fn description(&self) -> &str {
        self.description
    }

    /// Get the [PermissionType] for the [Command].
    pub fn permissions(&self) -> &PermissionType {
        &self.permissions
    }

    pub fn add_option(mut self, option: Option<'a>) -> Self {
        self.options.push(option);
        self
    }

    pub fn options(&self) -> &Vec<Option<'a>> {
        &self.options
    }

    pub fn add_variant(mut self, variant: Command<'a>) -> Self {
        self.variants.push(variant);
        self
    }

    pub fn variants(&self) -> &Vec<Command<'a>> {
        &self.variants
    }

    /// Run the [ActionRoutine] for this [Command].
    pub async fn run(
        &self,
        ctx: &Context,
        command: &mut ApplicationCommandInteraction,
    ) -> crate::Result {
        if let Some(action) = &self.action {
            (action)(ctx, command).await
        } else {
            Err(Error::MissingActionRoutine)
        }
    }
}

pub struct Option<'a> {
    name: &'a str,
    description: &'a str,
    kind: CommandOptionType,
    required: bool,
}

impl<'a> Option<'a> {
    pub fn new(
        name: &'a str,
        description: &'a str,
        kind: CommandOptionType,
        required: bool,
    ) -> Result<Self, crate::Error> {
        if kind == CommandOptionType::SubCommand
            || kind == CommandOptionType::SubCommandGroup
            || kind == CommandOptionType::Unknown
        {
            panic!("Invalid command option type: {:?}", kind);
        }

        Ok(Self {
            name,
            description,
            kind,
            required,
        })
    }

    pub fn name(&self) -> &'a str {
        self.name
    }

    pub fn description(&self) -> &'a str {
        self.description
    }

    pub fn kind(&self) -> CommandOptionType {
        self.kind
    }

    pub fn required(&self) -> bool {
        self.required
    }
}
