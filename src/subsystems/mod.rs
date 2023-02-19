use serenity::{
    async_trait,
    model::prelude::{GuildChannel, Message, Presence, Ready},
    prelude::Context,
};

use crate::command::Command;

pub mod events;
pub mod memes;
mod status_meaning;
mod stream_indicator;
pub mod thread_reviver;

pub fn subsystems() -> Vec<Box<dyn Subsystem>> {
    vec![
        Box::new(events::Events),
        Box::new(memes::Memes),
        Box::new(status_meaning::StatusMeaning),
        Box::new(stream_indicator::StreamIndicator),
        Box::new(thread_reviver::ThreadReviver),
    ]
}

#[async_trait]
pub trait Subsystem: Send + Sync {
    fn generate_commands(&self) -> Vec<Command<'static>>;

    async fn ready(&self, _ctx: &Context, _ready: &Ready) {}
    async fn message(&self, _ctx: &Context, _message: &Message) {}
    async fn presence(&self, _ctx: &Context, _new_data: &Presence) {}
    async fn thread(&self, _ctx: &Context, _thread: &GuildChannel) {}
}
