use std::collections::HashMap;
use std::{env, fs};

use log::error;

use serde::{Deserialize, Serialize};
use serenity::client::{Client, ClientBuilder};
use serenity::model::prelude::{GuildId, UserId};
use serenity::prelude::{GatewayIntents, TypeMapKey};

#[derive(Deserialize, Serialize)]
pub struct Config {
    manager: UserId,
    status_meaning: Option<String>,
    tokens: Tokens,
    guilds: Option<HashMap<GuildId, Guild>>,
}

#[derive(Deserialize, Serialize)]
struct Tokens {
    discord: String,
}

#[derive(Deserialize, Serialize)]
struct Guild {
    trigger_map: HashMap<String, String>,
}

impl TypeMapKey for Config {
    type Value = Config;
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

    fn save(&self) {
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

    pub fn get_status_meaning(&self) -> Option<String> {
        self.status_meaning.clone()
    }

    /// Construct a [ClientBuilder] from the supplied
    /// [GatewayIntents] and the configured Discord token.
    pub fn discord_client(&self, intents: GatewayIntents) -> ClientBuilder {
        Client::builder(&self.tokens.discord, intents)
    }
}
