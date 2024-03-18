use std::{fmt::Display, str::FromStr};

use serde::{Deserialize, Serialize};
use serenity::{async_trait, model::prelude::Ready, prelude::Context};
use tinyvec::ArrayVec;

use crate::{
    command::{notify_subscribers, Command, Option, OptionType, PermissionType},
    config::Config,
    create_raw_embed, ActionResponse, Error,
};

use super::Subsystem;

const EVENTS: [Event; 3] = [Event::Startup, Event::Stream, Event::Error];

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Hash, Clone, Copy)]
pub enum Event {
    Startup,
    Stream,
    Error,
}

impl Display for Event {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                Self::Startup => "Startup",
                Self::Stream => "Streaming",
                Self::Error => "Error",
            }
        )
    }
}

impl FromStr for Event {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if let Some(event) = EVENTS.iter().find(|e| e.to_string() == s) {
            Ok(*event)
        } else {
            Err(Error::InvalidEvent(format!(
                "Unknown string representation of Event: {s}"
            )))
        }
    }
}

pub struct Events;

#[async_trait]
impl Subsystem for Events {
    fn generate_commands(&self) -> Vec<Command<'static>> {
        let options = Box::new(
            EVENTS
                .iter()
                .map(|e| e.to_string())
                .collect::<ArrayVec<[String; 25]>>(),
        );

        vec![Command::new(
            "events",
            "Manage subscriptions to notifications for specific bot events.",
            PermissionType::Universal,
            None,
        )
        .add_variant(
            Command::new(
                "subscribe",
                "Subscribe to a bot event. Some events may be restricted.",
                PermissionType::Universal,
                Some(Box::new(move |ctx, command, params| {
                    Box::pin(async {
                        let event = get_param!(params, String, "event");
                        let event = Event::from_str(event)?;
                        let mut data = crate::acquire_data_handle!(write ctx);
                        let config = data.get_mut::<Config>().unwrap();
                        let subscribers = config.subscribers_mut(event);
                        Ok(Some(if !subscribers.contains(&command.user.id) {
                            subscribers.push(command.user.id);
                            config.save();
                            ActionResponse::new(
                                create_raw_embed(format!("Successfully subscribed to {event}.")),
                                true,
                            )
                        } else {
                            ActionResponse::new(
                                create_raw_embed(format!("You're already subscribed to {event}.")),
                                true,
                            )
                        }))
                    })
                })),
            )
            .add_option(Option::new(
                "event",
                "The event type you'd like to subscribe to.",
                OptionType::StringSelect(options.clone()),
                true,
            )),
        )
        .add_variant(
            Command::new(
                "unsubscribe",
                "Unsubscribe from a bot event.",
                PermissionType::Universal,
                Some(Box::new(move |ctx, command, params| {
                    Box::pin(async {
                        let event = get_param!(params, String, "event");
                        let event = Event::from_str(event)?;
                        let mut data = crate::acquire_data_handle!(write ctx);
                        let config = data.get_mut::<Config>().unwrap();
                        let subscribers = config.subscribers_mut(event);
                        Ok(Some(if subscribers.contains(&command.user.id) {
                            subscribers.retain(|u| *u != command.user.id);
                            config.save();
                            ActionResponse::new(
                                create_raw_embed(format!(
                                    "Successfully unsubscribed from {event}."
                                )),
                                true,
                            )
                        } else {
                            ActionResponse::new(
                                create_raw_embed(format!("You aren't subscribed to {event}.")),
                                true,
                            )
                        }))
                    })
                })),
            )
            .add_option(Option::new(
                "event",
                "The event type you'd like to unsubscribe from.",
                OptionType::StringSelect(options),
                true,
            )),
        )]
    }

    async fn ready(&self, ctx: &Context, _ready: &Ready) {
        notify_subscribers(
            ctx,
            Event::Startup,
            format!(
                "**Hey!**
I'm starting up with version [{}]({}/releases/tag/v{}). 😁",
                crate::VERSION,
                crate::GITHUB_URL,
                crate::VERSION,
            )
            .as_str(),
        )
        .await;
    }
}
