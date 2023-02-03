use std::collections::HashMap;
use std::{env, fs};

use chrono::{Days, Utc};
use log::error;

use serde::{Deserialize, Serialize};
use serenity::client::{Client, ClientBuilder};
use serenity::model::prelude::{ChannelId, GuildId, Message, MessageId, UserId};
use serenity::prelude::{GatewayIntents, TypeMapKey};

#[derive(Deserialize, Serialize)]
pub struct Config {
    manager: UserId,
    status_meaning: Option<String>,
    tokens: Tokens,
    /// Maps a [String]-encoded [GuildId] to its respective [Guild].
    /// Using a [String] here as [toml] has issues deserialising this to
    /// anything else, for some reason?
    guilds: Option<HashMap<String, Guild>>,
}

impl Config {
    /// Load config from the configuration file, located either at
    /// the location specified by the `LOKI_CONFIG_PATH` environment
    /// variable or `config.toml` by default.
    pub fn load() -> Self {
        let config_path = env::var("LOKI_CONFIG_PATH").unwrap_or("config.toml".to_string());

        let config = match fs::read_to_string(&config_path) {
            Ok(s) => s,
            Err(e) => panic!("Unable to read config at '{}': {:?}", &config_path, e),
        };
        let mut config: Self = toml::from_str(&config).unwrap();
        if let None = config.guilds {
            config.guilds = Some(HashMap::new());
        }
        config
    }

    pub fn save(&self) {
        let config_path = env::var("LOKI_CONFIG_PATH").unwrap_or("config.toml".to_string());

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

    pub fn guild(&self, id: &GuildId) -> Option<&Guild> {
        if let Some(guilds) = &self.guilds {
            if !guilds.contains_key(&id.to_string()) {
                return None;
            }
            return Some(guilds.get(&id.to_string()).unwrap());
        } else {
            unreachable!()
        }
    }

    pub fn guild_mut(&mut self, id: &GuildId) -> &mut Guild {
        if let Some(guilds) = &mut self.guilds {
            if !guilds.contains_key(&id.to_string()) {
                guilds.insert(id.to_string(), Guild::default());
            }
            return guilds.get_mut(&id.to_string()).unwrap();
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

    pub fn get_memes_channel(&self) -> Option<ChannelId> {
        if let Some(memes) = &self.memes {
            Some(memes.channel)
        } else {
            None
        }
    }

    pub fn get_memes_reset_time(&self) -> Option<chrono::DateTime<Utc>> {
        if let Some(memes) = &self.memes {
            memes.last_reset.checked_add_days(Days::new(7))
        } else {
            None
        }
    }

    pub fn memes_reset(&mut self) -> Option<Message> {
        if let Some(memes) = &mut self.memes {
            // todo: calculate winner...
            memes.last_reset = Utc::now();
            memes.memes_list.clear();
            // todo: return winner
            None
        } else {
            None
        }
    }
}

#[derive(Deserialize, Serialize, Clone)]
struct Memes {
    channel: ChannelId,
    last_reset: chrono::DateTime<Utc>,
    memes_list: Vec<MessageId>,
}

impl Memes {
    fn new(channel: ChannelId) -> Self {
        Self {
            channel,
            last_reset: Utc::now(),
            memes_list: Vec::new(),
        }
    }
}
