use serenity::{
    async_trait,
    model::prelude::{GuildChannel, Member, Message, Presence, Ready},
    prelude::Context,
};

use crate::command::Command;

pub mod events;
pub mod memes;
mod status_meaning;
mod stream_indicator;
mod text_response;
pub mod thread_reviver;
pub mod timeout_monitor;

pub fn subsystems() -> Vec<Box<dyn Subsystem>> {
    vec![
        Box::new(events::Events),
        Box::new(memes::Memes),
        Box::new(status_meaning::StatusMeaning),
        Box::new(stream_indicator::StreamIndicator),
        Box::new(text_response::TextResponse),
        Box::new(thread_reviver::ThreadReviver),
        Box::new(timeout_monitor::TimeoutMonitor),
    ]
}

#[async_trait]
pub trait Subsystem: Send + Sync {
    fn generate_commands(&self) -> Vec<Command<'static>>;

    async fn ready(&self, _ctx: &Context, _ready: &Ready) {}
    async fn message(&self, _ctx: &Context, _message: &Message) {}
    async fn presence(&self, _ctx: &Context, _new_data: &Presence) {}
    async fn thread(&self, _ctx: &Context, _thread: &GuildChannel) {}
    async fn member(&self, _ctx: &Context, _old: &Option<Member>, _new: &Member) {}
}
