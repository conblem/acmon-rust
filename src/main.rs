use anyhow::{anyhow, Result};
use tokio::runtime::Runtime;
use tracing::{instrument, Instrument};

use config::Config;
use server::{AcmeServer, AcmeServerBuilder, ApiDirectory, ProxyAcmeServer};

mod config;
mod http;
mod repo;
mod server;

#[instrument]
fn main() -> Result<()> {
    match tracing_subscriber::fmt::try_init() {
        Err(e) => return Err(anyhow!(e)),
        Ok(()) => {}
    };

    let config = config::load_config()?;
    let runtime = Runtime::new()?;

    let block = async move {
        let acme_server = ProxyAcmeServer::builder().le_staging().build().await?;

        let http_server = http::run(&config, acme_server);
        tokio::spawn(http_server).await?
    };

    runtime.block_on(block.in_current_span())
}
