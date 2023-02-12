use std::collections::hash_map::Keys;
use std::collections::HashMap;
use std::{env, fs};
use tokio::sync::RwLockReadGuard;

use chrono::{Days, Utc};
use log::error;

use serde::{Deserialize, Serialize};
use serenity::client::{Client, ClientBuilder};
use serenity::model::prelude::{ChannelId, GuildId, MessageId, UserId};
use serenity::prelude::{GatewayIntents, TypeMap, TypeMapKey};

use crate::subsystems::events::Event;

/// Abstraction to try get a handle to a [GuildId]'s [Guild] entry
/// from the config, based on a [RwLockReadGuard<TypeMap>] obtained
/// from a [serenity::prelude::Context].
pub fn get_guild<'a>(data: &'a RwLockReadGuard<TypeMap>, guild: &GuildId) -> Option<&'a Guild> {
    let config = data.get::<Config>().unwrap();
    config.guild(guild)
}

/// Abstraction to try get a handle to a [GuildId]'s [Memes] entry
/// from the config, based on a [RwLockReadGuard<TypeMap>] obtained
/// from a [serenity::prelude::Context].
///
/// In particular, this function helps to avoid a double-nested `if`.
pub fn get_memes<'a>(data: &'a RwLockReadGuard<TypeMap>, guild: &GuildId) -> Option<&'a Memes> {
    if let Some(guild) = get_guild(data, guild) {
        guild.memes()
    } else {
        None
    }
}

#[derive(Deserialize, Serialize)]
pub struct Config {
    manager: UserId,
    status_meaning: Option<String>,
    tokens: Tokens,
    /// Maps a [String]-encoded [GuildId] to its respective [Guild].
    /// Using a [String] here as [toml] has issues deserialising this to
    /// anything else, for some reason?
    guilds: Option<HashMap<String, Guild>>,
    subscribers: Option<HashMap<crate::subsystems::events::Event, Vec<UserId>>>,
}

impl Config {
    /// Load config from the configuration file, located either at
    /// the location specified by the `LOKI_CONFIG_PATH` environment
    /// variable or `config.toml` by default.
    pub fn load() -> Self {
        let config_path =
            env::var("LOKI_CONFIG_PATH").unwrap_or_else(|_| "config.toml".to_string());

        let config = match fs::read_to_string(&config_path) {
            Ok(s) => s,
            Err(e) => panic!("Unable to read config at '{}': {:?}", &config_path, e),
        };
        let mut config: Self = toml::from_str(&config).unwrap();
        if config.guilds.is_none() {
            config.guilds = Some(HashMap::new());
        }
        if config.subscribers.is_none() {
            config.subscribers = Some(HashMap::new());
        }
        config
    }

    pub fn save(&self) {
        let config_path =
            env::var("LOKI_CONFIG_PATH").unwrap_or_else(|_| "config.toml".to_string());

        match toml::to_string_pretty(self) {
            Ok(s) => {
                if let Err(e) = fs::write(config_path.clone(), s) {
                    error!("Failed to write config to {config_path}: {e}");
                }
            }
            Err(e) => error!("Failed to serialise config: {e}"),
        }
    }

    pub fn get_manager(&self) -> UserId {
        self.manager
    }

    pub fn guilds(&self) -> Keys<'_, std::string::String, Guild> {
        if let Some(guilds) = &self.guilds {
            guilds.keys()
        } else {
            unreachable!();
        }
    }

    pub fn guild(&self, id: &GuildId) -> Option<&Guild> {
        if let Some(guilds) = &self.guilds {
            guilds.get(&id.to_string())
        } else {
            unreachable!()
        }
    }

    pub fn guild_mut(&mut self, id: &GuildId) -> &mut Guild {
        if let Some(guilds) = &mut self.guilds {
            guilds.entry(id.to_string()).or_insert_with(Guild::default)
        } else {
            unreachable!()
        }
    }

    pub fn subscribers(&self, event: Event) -> Option<&Vec<UserId>> {
        if let Some(subscribers) = &self.subscribers {
            subscribers.get(&event)
        } else {
            unreachable!()
        }
    }

    pub fn subscribers_mut(&mut self, event: Event) -> &mut Vec<UserId> {
        if let Some(subscribers) = &mut self.subscribers {
            subscribers.entry(event).or_insert_with(Vec::new)
        } else {
            unreachable!()
        }
    }

    pub fn get_status_meaning(&self) -> Option<String> {
        self.status_meaning.clone()
    }

    pub fn set_status_meaning(&mut self, s: Option<String>) {
        self.status_meaning = s;
        self.save();
    }

    /// Construct a [ClientBuilder] from the supplied
    /// [GatewayIntents] and the configured Discord token.
    pub fn discord_client(&self, intents: GatewayIntents) -> ClientBuilder {
        Client::builder(&self.tokens.discord, intents)
    }
}

impl TypeMapKey for Config {
    type Value = Config;
}

#[derive(Deserialize, Serialize)]
struct Tokens {
    discord: String,
}

#[derive(Deserialize, Serialize, Default)]
pub struct Guild {
    trigger_map: Option<HashMap<String, String>>,
    memes: Option<Memes>,
}

impl Guild {
    pub fn set_memes_channel(&mut self, channel: Option<ChannelId>) {
        if let Some(channel) = channel {
            self.memes = Some(Memes::new(channel));
        } else {
            self.memes = None;
        }
    }

    pub fn memes(&self) -> Option<&Memes> {
        if let Some(memes) = &self.memes {
            Some(memes)
        } else {
            None
        }
    }

    pub fn memes_mut(&mut self) -> Option<&mut Memes> {
        if let Some(memes) = &mut self.memes {
            Some(memes)
        } else {
            None
        }
    }
}

#[derive(Deserialize, Serialize, Clone)]
pub struct Memes {
    channel: ChannelId,
    last_reset: chrono::DateTime<Utc>,
    memes_list: Vec<MessageId>,
    reacted: bool,
}

impl Memes {
    fn new(channel: ChannelId) -> Self {
        Self {
            channel,
            last_reset: Utc::now(),
            memes_list: Vec::new(),
            reacted: false,
        }
    }

    pub fn list(&self) -> &Vec<MessageId> {
        &self.memes_list
    }

    pub fn add(&mut self, message: MessageId) {
        self.memes_list.push(message);
    }

    pub fn next_reset(&self) -> chrono::DateTime<Utc> {
        self.last_reset.checked_add_days(Days::new(7)).unwrap()
    }

    pub fn reset(&mut self) {
        self.last_reset = Utc::now();
        self.memes_list.clear();
        self.reacted = false;
    }

    pub fn channel(&self) -> ChannelId {
        self.channel
    }

    pub fn has_reacted(&self) -> bool {
        self.reacted
    }

    pub fn reacted(&mut self) {
        self.reacted = true;
    }
}
