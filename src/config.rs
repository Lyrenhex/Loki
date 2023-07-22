use std::collections::hash_map::Keys;
use std::collections::HashMap;
use std::{env, fs};
use tokio::sync::RwLockReadGuard;

use log::error;

use serde::{Deserialize, Serialize};
use serenity::client::{Client, ClientBuilder};
use serenity::model::prelude::{Channel, GuildId, UserId};
use serenity::prelude::{GatewayIntents, TypeMap, TypeMapKey};

#[cfg(feature = "events")]
use crate::subsystems::events::Event;
#[cfg(feature = "memes")]
use crate::subsystems::memes::Memes;
#[cfg(feature = "timeout-monitor")]
use crate::subsystems::timeout_monitor::{
    AnnouncementsConfig as TimeoutAnnouncementsConfig, UserTimeoutData,
};
#[cfg(feature = "memes")]
use serenity::model::prelude::ChannelId;

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
#[cfg(feature = "memes")]
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
    #[cfg(feature = "events")]
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
        #[cfg(feature = "events")]
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

    /// Construct a [ClientBuilder] from the supplied
    /// [GatewayIntents] and the configured Discord token.
    pub fn discord_client(&self, intents: GatewayIntents) -> ClientBuilder {
        Client::builder(&self.tokens.discord, intents)
    }
}

#[cfg(feature = "events")]
impl Config {
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
}

#[cfg(feature = "status-meaning")]
impl Config {
    pub fn get_status_meaning(&self) -> Option<String> {
        self.status_meaning.clone()
    }

    pub fn set_status_meaning(&mut self, s: Option<String>) {
        self.status_meaning = s;
        self.save();
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
    #[serde(skip)]
    threads_started: bool,
    response_map: Option<HashMap<String, String>>,
    #[cfg(feature = "memes")]
    memes: Option<Memes>,
    #[cfg(feature = "timeout-monitor")]
    timeouts: Option<HashMap<String, UserTimeoutData>>,
    #[cfg(feature = "timeout-monitor")]
    timeouts_announcement_config: Option<TimeoutAnnouncementsConfig>,
}

impl Guild {
    pub fn threads_started(&self) -> bool {
        self.threads_started
    }

    pub fn set_threads_started(&mut self) {
        self.threads_started = true;
    }

    pub fn response_map_mut(&mut self) -> &mut HashMap<String, String> {
        if self.response_map.is_none() {
            self.response_map = Some(HashMap::new());
        }
        self.response_map.as_mut().unwrap()
    }

    pub fn response_map(&self) -> &Option<HashMap<String, String>> {
        &self.response_map
    }
}

#[cfg(feature = "memes")]
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

#[cfg(feature = "timeout-monitor")]
impl Guild {
    pub fn timeouts_mut(&mut self) -> &mut HashMap<String, UserTimeoutData> {
        if self.timeouts.is_none() {
            self.timeouts = Some(HashMap::new());
        }
        self.timeouts.as_mut().unwrap()
    }

    pub fn timeouts(&self) -> &Option<HashMap<String, UserTimeoutData>> {
        &self.timeouts
    }

    pub fn timeouts_announcement_init(&mut self, channel: Channel) {
        if let Some(_) = self.timeouts_announcement_config {
            error!("Attempted to initialise timeout announcement subsystem when it's already initialised!");
            return;
        }
        self.timeouts_announcement_config = Some(TimeoutAnnouncementsConfig::new(channel));
    }

    pub fn timeouts_announcement_uninit(&mut self) {
        self.timeouts_announcement_config = None;
    }

    pub fn timeouts_announcement_config_mut(&mut self) -> Option<&mut TimeoutAnnouncementsConfig> {
        self.timeouts_announcement_config.as_mut()
    }

    pub fn timeouts_announcement_config(&self) -> Option<&TimeoutAnnouncementsConfig> {
        self.timeouts_announcement_config.as_ref()
    }
}
