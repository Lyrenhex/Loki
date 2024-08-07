mod util;

use tinyvec::ArrayVec;
pub use util::*;

use std::{pin::Pin, sync::Arc};

use serenity::{
    all::{CommandDataOption, CreateEmbed},
    model::{
        application::{CommandInteraction, CommandOptionType},
        prelude::ChannelType,
        Permissions,
    },
    prelude::Context,
};

use crate::Error;

const MIN_NUM: i64 = -(MAX_NUM);
const MAX_NUM: i64 = 1 << 54;

pub const NUM_SELECTABLES: usize = 25;

type ActionRoutine = Box<
    dyn (for<'b> Fn(
            &'b Context,
            &'b mut CommandInteraction,
            &'b Vec<CommandDataOption>,
        ) -> Pin<
            Box<
                dyn std::future::Future<Output = crate::Result<std::option::Option<ActionResponse>>>
                    + Send
                    + 'b,
            >,
        >) + Sync
        + Send,
>;

pub struct ActionResponse {
    embed: CreateEmbed,
    ephemeral: bool,
}

impl ActionResponse {
    pub fn new(embed: CreateEmbed, ephemeral: bool) -> Self {
        Self { embed, ephemeral }
    }

    pub fn embed(self) -> CreateEmbed {
        self.embed
    }

    pub fn ephemeral(&self) -> bool {
        self.ephemeral
    }
}

#[derive(Debug, PartialEq, Eq, Clone)]
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
#[derive(Clone)]
pub struct Command<'a> {
    name: &'a str,
    description: &'a str,
    permissions: PermissionType,
    options: Vec<Option<'a>>,
    variants: Vec<Command<'a>>,
    action: Arc<std::option::Option<ActionRoutine>>,
    global: bool,
}

impl<'a> Command<'a> {
    /// Construct a new global Command with the given name and description,
    /// which performs the given [ActionRoutine] when called.
    ///
    /// ## Example
    ///
    /// ```
    /// use loki_discord_bot::{PermissionType, Command};
    ///
    /// let _command = Command::new(
    ///     "name",
    ///     "A description of what the command does.",
    ///     PermissionType::Universal,
    ///     Some(
    ///         Box::new(move |ctx, command, params| {
    ///             Box::pin(async {
    ///                 // do something here
    ///                 Ok(None) // no response needed
    ///             })
    ///         })
    ///     ),
    /// );
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
            action: Arc::new(action),
            global: true,
        }
    }

    pub fn new_stub(name: &'a str, action: std::option::Option<ActionRoutine>) -> Self {
        Self {
            name,
            description: "",
            permissions: PermissionType::Universal,
            options: Vec::new(),
            variants: Vec::new(),
            action: Arc::new(action),
            global: false,
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

    pub fn global(&self) -> bool {
        self.global
    }

    pub fn add_option(mut self, option: Option<'a>) -> Self {
        self.options.push(option);
        self
    }

    pub fn options(&self) -> &Vec<Option<'a>> {
        &self.options
    }

    pub fn add_variant(mut self, variant: Command<'a>) -> Self {
        assert_eq!(variant.global(), self.global);
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
        command: &mut CommandInteraction,
        params: &Vec<CommandDataOption>,
    ) -> crate::Result<std::option::Option<ActionResponse>> {
        if let Some(action) = &*self.action {
            (action)(ctx, command, params).await
        } else {
            Err(Error::MissingActionRoutine)
        }
    }
}

#[derive(Debug, Clone)]
pub struct Option<'a> {
    name: &'a str,
    description: &'a str,
    kind: OptionType,
    required: bool,
}

impl<'a> Option<'a> {
    pub fn new(name: &'a str, description: &'a str, kind: OptionType, required: bool) -> Self {
        match kind.clone() {
            OptionType::StringInput(min, max) => {
                if let Some(min) = min {
                    if min > 6000 {
                        panic!("StringInput minimum value above 6000: {min}");
                    }
                }
                if let Some(max) = max {
                    if !(1..=6000).contains(&max) {
                        panic!("StringInput maximum value out of bounds (should be between 1 and 6000): {max}");
                    }
                }
            }
            OptionType::StringSelect(options) => {
                if options.is_empty() {
                    panic!("No choices for StringSelect!");
                }
                if options.len() > NUM_SELECTABLES {
                    panic!(
                        "More than {NUM_SELECTABLES} choices for StringSelect: {:?}",
                        options
                    );
                }
            }
            OptionType::IntegerInput(min, max) => {
                if let Some(min) = min {
                    if min < MIN_NUM {
                        panic!("Integer minimum value below -2^53: {min}");
                    }
                }
                if let Some(max) = max {
                    if max > MAX_NUM {
                        panic!("Integer maximum value above 2^53: {max}");
                    }
                }
            }
            OptionType::IntegerSelect(options) => {
                if options.is_empty() {
                    panic!("No choices for IntegerSelect!");
                }
                if options.len() > NUM_SELECTABLES {
                    panic!(
                        "More than {NUM_SELECTABLES} choices for IntegerSelect: {:?}",
                        options
                    );
                }
                options
                    .iter()
                    .for_each(|x| assert!((&MIN_NUM..&MAX_NUM).contains(&x)));
            }
            OptionType::NumberInput(min, max) => {
                if let Some(min) = min {
                    if min < MIN_NUM as f64 {
                        panic!("Number minimum value below -2^53: {min}");
                    }
                }
                if let Some(max) = max {
                    if max > MAX_NUM as f64 {
                        panic!("Number maximum value above 2^53: {max}");
                    }
                }
            }
            OptionType::NumberSelect(options) => {
                if options.is_empty() {
                    panic!("No choices for IntegerSelect!");
                }
                if options.len() > NUM_SELECTABLES {
                    panic!(
                        "More than {NUM_SELECTABLES} choices for IntegerSelect: {:?}",
                        options
                    );
                }
                options
                    .iter()
                    .for_each(|x| assert!(x >= &(MIN_NUM as f64) && x <= &(MAX_NUM as f64)));
            }
            OptionType::Boolean
            | OptionType::User
            | OptionType::Channel(_)
            | OptionType::Role
            | OptionType::Mentionable
            | OptionType::Attachment => {}
        }
        Self {
            name,
            description,
            kind,
            required,
        }
    }

    pub fn name(&self) -> &'a str {
        self.name
    }

    pub fn description(&self) -> &'a str {
        self.description
    }

    pub fn kind(&self) -> OptionType {
        self.kind.clone()
    }

    pub fn required(&self) -> bool {
        self.required
    }
}

#[derive(Debug, Clone)]
pub enum OptionType {
    /// A String input based on the given range (min, max).
    /// Limited to ([0..6000], [1..6000])
    StringInput(std::option::Option<u16>, std::option::Option<u16>),
    /// A String input based on the given options.
    StringSelect(Box<ArrayVec<[String; NUM_SELECTABLES]>>),
    /// An integer input, optionally limited to a specific range.
    /// Note that integers must be between -2^53 and 2^53.
    IntegerInput(std::option::Option<i64>, std::option::Option<i64>),
    /// An integer select.
    /// Note that integers must be between -2^53 and 2^53.
    IntegerSelect(ArrayVec<[i64; NUM_SELECTABLES]>),
    Boolean,
    User,
    Channel(std::option::Option<Vec<ChannelType>>),
    Role,
    Mentionable,
    /// A double input, optionally limited to a specific range.
    /// Note that numbers must be between -2^53 and 2^53.
    NumberInput(std::option::Option<f64>, std::option::Option<f64>),
    /// A number (double) selection.
    /// Note that numbers must be between -2^53 and 2^53.
    NumberSelect(ArrayVec<[f64; NUM_SELECTABLES]>),
    Attachment,
}

impl From<OptionType> for CommandOptionType {
    fn from(ot: OptionType) -> Self {
        match ot {
            OptionType::StringInput(_, _) => CommandOptionType::String,
            OptionType::StringSelect(_) => CommandOptionType::String,
            OptionType::IntegerInput(_, _) => CommandOptionType::Integer,
            OptionType::IntegerSelect(_) => CommandOptionType::Integer,
            OptionType::Boolean => CommandOptionType::Boolean,
            OptionType::User => CommandOptionType::User,
            OptionType::Channel(_) => CommandOptionType::Channel,
            OptionType::Role => CommandOptionType::Role,
            OptionType::Mentionable => CommandOptionType::Mentionable,
            OptionType::NumberInput(_, _) => CommandOptionType::Number,
            OptionType::NumberSelect(_) => CommandOptionType::Number,
            OptionType::Attachment => CommandOptionType::Attachment,
        }
    }
}
