use serenity::{
    async_trait,
    model::prelude::{Guild, Message, Presence, Ready},
    prelude::Context,
};

use crate::command::Command;

pub mod events;
mod memes;
mod status_meaning;
mod stream_indicator;

pub fn subsystems() -> [Box<dyn Subsystem>; 4] {
    [
        Box::new(events::Events),
        Box::new(memes::Memes),
        Box::new(status_meaning::StatusMeaning),
        Box::new(stream_indicator::StreamIndicator),
    ]
}

#[async_trait]
pub trait Subsystem: Send + Sync {
    fn generate_commands(&self) -> Vec<Command<'static>>;

    async fn guild_init(&self, _ctx: Context, _g: Guild) {}

    async fn ready(&self, _ctx: &Context, _ready: &Ready) {}
    async fn message(&self, _ctx: &Context, _message: &Message) {}
    async fn presence(&self, _ctx: &Context, _new_data: &Presence) {}
}
