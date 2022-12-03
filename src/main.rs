use serenity::async_trait;
use serenity::client::{Client, Context, EventHandler};
use serenity::model::prelude::Member;
use serenity::model::prelude::UserId;
use serenity::prelude::GatewayIntents;

use std::collections::HashMap;
use std::env;

struct Handler {
    names: HashMap<UserId, String>,
}

impl Handler {
    pub fn new() -> Self {
        Self {
            names: HashMap::new(),
        }
    }
}

#[async_trait]
impl EventHandler for Handler {
    // Note: This event will not trigger unless the "guild members" privileged intent is enabled on the bot application page.
    async fn guild_member_update(&self, context: Context, _old: Option<Member>, new: Member) {
        if !self.names.contains_key(&new.user.id) {
            return;
        }

        if let Err(e) = new
            .edit(context.http.clone(), |m| {
                m.nickname(self.names.get(&new.user.id).unwrap())
            })
            .await
        {
            eprintln!(
                "Error changing nickname of {} ({}) in guild {}: {}",
                new.user.id, new.user.name, new.guild_id, e
            )
        }
    }
}

#[tokio::main]
async fn main() {
    // Login with a bot token from the environment
    let token = env::var("LOKI_DISCORD_TOKEN").expect("token");
    let handler = Handler::new();
    let mut client = Client::builder(token, GatewayIntents::non_privileged())
        .event_handler(handler)
        .await
        .expect("Error creating client");

    loop {
        // start listening for events by starting a single shard
        if let Err(err) = client.start().await {
            match err {
                _ => {
                    // unknown error (fatal): announce and terminate.
                    eprintln!("[FATAL] ERROR: {:?}", err);
                    break;
                }
            }
        }
    }
}
