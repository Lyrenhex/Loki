use serenity::{
    async_trait,
    model::prelude::{GuildChannel, Member, Message, Presence, Ready},
    prelude::Context,
};

use crate::command::Command;

#[cfg(feature = "events")]
pub mod events;
#[cfg(feature = "memes")]
pub mod memes;
#[cfg(feature = "nickname-lottery")]
pub mod nickname_lottery;
#[cfg(feature = "status-meaning")]
mod status_meaning;
#[cfg(feature = "stream-indicator")]
mod stream_indicator;
#[cfg(feature = "text-response")]
mod text_response;
#[cfg(feature = "thread-reviver")]
pub mod thread_reviver;
#[cfg(feature = "timeout-monitor")]
pub mod timeout_monitor;

pub fn subsystems() -> Vec<Box<dyn Subsystem>> {
    vec![
        #[cfg(feature = "events")]
        Box::new(events::Events),
        #[cfg(feature = "memes")]
        Box::new(memes::MemesVoting),
        #[cfg(feature = "nickname-lottery")]
        Box::new(nickname_lottery::NicknameLottery),
        #[cfg(feature = "status-meaning")]
        Box::new(status_meaning::StatusMeaning),
        #[cfg(feature = "stream-indicator")]
        Box::new(stream_indicator::StreamIndicator),
        #[cfg(feature = "text-response")]
        Box::new(text_response::TextResponse),
        #[cfg(feature = "thread-reviver")]
        Box::new(thread_reviver::ThreadReviver),
        #[cfg(feature = "timeout-monitor")]
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
