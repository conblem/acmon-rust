use anyhow::{Result, anyhow};
use tokio::runtime::Runtime;
use tracing::{Instrument, instrument};

use server::{AcmeServer, AcmeServerBuilder, ProxyAcmeServer};

mod config;
mod http;
mod limiter;
mod repo;
mod server;

#[instrument]
fn main() -> Result<()> {
    match tracing_subscriber::fmt::try_init() {
        Err(e) => Err(anyhow!(e))?,
        Ok(()) => {}
    };

    let _config = config::load_config()?;
    let runtime = Runtime::new()?;

    let block = async move {
        let acme_server = ProxyAcmeServer::builder().le_staging().build().await?;
        acme_server.get_nonce().await?;

        Ok(()) as Result<()>
    };

    runtime.block_on(block.in_current_span())
}
