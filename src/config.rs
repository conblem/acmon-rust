use anyhow::Result;
use serde::Deserialize;
use std::env;
use std::fs::read;
use tracing::{debug, info, instrument, Span};

#[derive(Deserialize, Debug)]
pub struct Config {
    pub db: Vec<String>,
    pub url: String,
}

const DEFAULT_CONFIG_PATH: &str = "config.yaml";

// not async so we can load Tokio runtime configuration
#[instrument(err, fields(config_path))]
pub(super) fn load_config() -> Result<Config> {
    let span = Span::current();
    let config_path = env::args().nth(1);
    let config_path = config_path.as_deref().unwrap_or(DEFAULT_CONFIG_PATH);

    span.record("config_path", &config_path);

    let file = read(config_path)?;
    debug!(file_length = file.len(), "Read file");

    let config: Config = serde_yaml::from_slice(&file)?;
    info!(?config, "Deserialized config");

    Ok(config)
}
