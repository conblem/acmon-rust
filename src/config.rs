use anyhow::Result;
use serde::Deserialize;
use std::env;
use std::fs::read;
use tracing::{debug, info, info_span};

#[derive(Deserialize, Debug)]
pub struct Config {
    pub db: String,
}

const DEFAULT_CONFIG_PATH: &str = "config.yaml";

// not async so we can load Tokio runtime configuration in the future
pub(super) fn load_config() -> Result<Config> {
    let config_path = env::args().nth(1);
    let config_path = config_path.as_deref().unwrap_or(DEFAULT_CONFIG_PATH);

    let span = info_span!("load_config", config_path);
    let _enter = span.enter();

    let file = read(config_path)?;
    debug!(file_lenght = file.len(), "Read file");

    let config: Config = serde_yaml::from_slice(&file)?;
    // redact db information
    // maybe make this optional
    let config_str = format!("{:?}", config).replace(&config.db, "******");
    info!(config = %config_str, "Deserialized config");

    Ok(config)
}
