use std::collections::HashMap;

use log::error;
use serenity::{
    async_trait,
    http::Http,
    model::prelude::{ChannelType, Guild, GuildChannel},
    prelude::Context,
};

use super::Subsystem;

struct ChannelError {
    public: bool,
    channel: String,
}

pub struct ThreadReviver;

#[async_trait]
impl Subsystem for ThreadReviver {
    fn generate_commands(&self) -> Vec<crate::command::Command<'static>> {
        vec![]
    }

    async fn thread(&self, ctx: &Context, thread: &GuildChannel) {
        Self::revive_thread(&ctx.http, thread).await;
    }
}

impl ThreadReviver {
    async fn revive_thread(http: &impl AsRef<Http>, thread: &GuildChannel) {
        if let Some(metadata) = thread.thread_metadata {
            if metadata.archived {
                let result = thread
                    .id
                    .edit_thread(http, |thread| thread.archived(false))
                    .await;
                match result {
                    Ok(_) => (),
                    Err(error) => error!(
                        "Failed to revive thread (does the bot have permission?): {}",
                        error
                    ),
                }
            }
        }
    }

    pub async fn guild_init(ctx: Context, g: Guild) {
        let mut channel_errors: HashMap<String, Vec<ChannelError>> = HashMap::new();
        for (channel_id, channel) in g.channels {
            if let Some(channel) = channel.guild() {
                if channel.kind == ChannelType::Text {
                    match channel_id
                        .get_archived_private_threads(&ctx.http, None, None)
                        .await
                    {
                        Ok(threads_data) => {
                            for thread in threads_data.threads {
                                Self::revive_thread(&ctx.http, &thread).await;
                            }
                        }
                        Err(error) => {
                            let vector = channel_errors.entry(error.to_string()).or_insert(vec![]);
                            vector.push(ChannelError {
                                public: false,
                                channel: channel.name.clone(),
                            });
                        }
                    };
                    match channel_id
                        .get_archived_public_threads(&ctx.http, None, None)
                        .await
                    {
                        Ok(threads_data) => {
                            for thread in threads_data.threads {
                                Self::revive_thread(&ctx.http, &thread).await;
                            }
                        }
                        Err(error) => {
                            let vector = channel_errors.entry(error.to_string()).or_insert(vec![]);
                            vector.push(ChannelError {
                                public: true,
                                channel: channel.name,
                            });
                        }
                    };
                }
            }
        }
        // print any errors we encountered in a reasonably nicely formatted way, to help with diagnosing either actual code issues or Discord permission issues.
        if !channel_errors.is_empty() {
            let mut err = format!("Errors retrieving threads in guild {}\n", g.id);
            for error in channel_errors.keys() {
                err += format!("\t{}:\n", error).as_str();
                for channel_error in channel_errors.get(error).unwrap() {
                    err += format!(
                        "\t\t{}\t{}\n",
                        if channel_error.public {
                            "public"
                        } else {
                            "private"
                        },
                        channel_error.channel
                    )
                    .as_str();
                }
            }
            error!("{}", err);
        }
    }
}
