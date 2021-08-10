use anyhow::Result;
use sqlx::AnyPool;
use tokio::runtime::Runtime;
use tracing::Instrument;

use server::{AcmeServer, AcmeServerBuilder, ProxyAcmeServer};

mod config;
mod http;
mod server;

#[tracing::instrument(err)]
fn main() -> Result<()> {
    // will panic if not successful
    tracing_subscriber::fmt::init();

    let config = config::load_config()?;
    let runtime = Runtime::new()?;

    let block = async move {
        let _pool = AnyPool::connect(&config.db).await?;

        let acme_server = ProxyAcmeServer::builder().le_staging().build().await?;
        acme_server.get_nonce().await?;

        Ok(()) as Result<()>
    };

    runtime.block_on(block.in_current_span())
}
