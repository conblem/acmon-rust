use anyhow::Result;
use serde::Deserialize;
use std::env;
use std::fs::read;
use tracing::{debug, info, info_span};

#[derive(Deserialize, Debug)]
pub struct Config {
    pub db: Vec<String>,
}

const DEFAULT_CONFIG_PATH: &str = "config.yaml";

// not async so we can load Tokio runtime configuration in the future
pub(super) fn load_config() -> Result<Config> {
    let config_path = env::args().nth(1);
    let config_path = config_path.as_deref().unwrap_or(DEFAULT_CONFIG_PATH);

    let span = info_span!("load_config", config_path);
    let _enter = span.enter();

    let file = read(config_path)?;
    debug!(file_length = file.len(), "Read file");

    let config: Config = serde_yaml::from_slice(&file)?;
    info!(?config, "Deserialized config");

    Ok(config)
}
