use crate::command::Command;

mod memes;

pub use memes::Memes;

pub trait Subsystem {
    /// Generate a top-level [Command] for the subsystem.
    /// All related commands for this subsystem should be managed
    /// by this [Command]'s subcommands.
    fn generate_command() -> Command<'static>;
}
