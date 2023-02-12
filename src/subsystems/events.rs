use std::{fmt::Display, str::FromStr};

use serde::{Deserialize, Serialize};
use tinyvec::ArrayVec;

use crate::{
    command::{create_response, Command, Option, OptionType, PermissionType},
    config::Config,
    Error,
};

const EVENTS: [Event; 2] = [Event::Startup, Event::Error];

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Hash, Clone, Copy)]
pub enum Event {
    Startup,
    Error,
}

impl Display for Event {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                Self::Startup => "Startup",
                Self::Error => "Error",
            }
        )
    }
}

impl FromStr for Event {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "Startup" => Ok(Self::Startup),
            "Error" => Ok(Self::Error),
            _ => Err(Error::InvalidEvent(format!(
                "Unknown string representation of Event: {s}"
            ))),
        }
    }
}

pub fn generate_command() -> Command<'static> {
    let options = EVENTS
        .iter()
        .map(|e| e.to_string())
        .collect::<ArrayVec<[String; 25]>>();

    Command::new(
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
            Some(Box::new(move |ctx, command| {
                Box::pin(async {
                    let event = command.data.options[0]
                        .options
                        .iter()
                        .find(|opt| opt.name == "event")
                        .unwrap()
                        .value
                        .as_ref()
                        .unwrap()
                        .as_str()
                        .unwrap();
                    let event = Event::from_str(event)?;
                    let mut data = ctx.data.write().await;
                    let config = data.get_mut::<Config>().unwrap();
                    let subscribers = config.subscribers_mut(event);
                    if !subscribers.contains(&command.user.id) {
                        subscribers.push(command.user.id);
                        config.save();
                        create_response(
                            &ctx.http,
                            command,
                            &format!("Successfully subscribed to {event}."),
                            true,
                        )
                        .await;
                    } else {
                        create_response(
                            &ctx.http,
                            command,
                            &format!("You're already subscribed to {event}."),
                            true,
                        )
                        .await;
                    }
                    Ok(())
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
            Some(Box::new(move |ctx, command| {
                Box::pin(async {
                    let event = command.data.options[0]
                        .options
                        .iter()
                        .find(|opt| opt.name == "event")
                        .unwrap()
                        .value
                        .as_ref()
                        .unwrap()
                        .as_str()
                        .unwrap();
                    let event = Event::from_str(event)?;
                    let mut data = ctx.data.write().await;
                    let config = data.get_mut::<Config>().unwrap();
                    let subscribers = config.subscribers_mut(event);
                    if subscribers.contains(&command.user.id) {
                        subscribers.retain(|u| *u != command.user.id);
                        config.save();
                        create_response(
                            &ctx.http,
                            command,
                            &format!("Successfully unsubscribed from {event}."),
                            true,
                        )
                        .await;
                    } else {
                        create_response(
                            &ctx.http,
                            command,
                            &format!("You aren't subscribed to {event}."),
                            true,
                        )
                        .await;
                    }
                    Ok(())
                })
            })),
        )
        .add_option(Option::new(
            "event",
            "The event type you'd like to unsubscribe from.",
            OptionType::StringSelect(options),
            true,
        )),
    )
}
