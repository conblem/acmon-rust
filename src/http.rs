use anyhow::{Error, Result};
use futures_util::future;
use futures_util::FutureExt;
use hyper::{Body, Response};
use std::convert::TryInto;
use std::fmt::{Debug, Formatter};
use std::future::Future;
use std::sync::Arc;
use warp::reject::Reject;
use warp::{Filter, Rejection};

use super::{AcmeServer, ApiDirectory, Config};

struct AnyhowRejection(Error);

impl AnyhowRejection {
    fn new(err: Error) -> Rejection {
        AnyhowRejection(err).into()
    }
}

impl Debug for AnyhowRejection {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl Reject for AnyhowRejection {}

async fn nonce<A: AcmeServer>(server: Arc<A>) -> Result<Response<Body>, Rejection> {
    async move {
        let nonce = server.get_nonce().await;
        let nonce = nonce.map_err(Into::into)?;

        let body = Response::builder()
            .header("nonce", nonce)
            .body(Body::empty())?;
        Ok(body)
    }
    .await
    .map_err(AnyhowRejection::new)
}

fn generate_directory(url: &str) -> Result<ApiDirectory, Error> {
    let key_change = format!("{}/acme/key_change", url).try_into()?;
    let new_order = format!("{}/acme/new_order", url).try_into()?;
    let new_account = format!("{}/acme/new_account", url).try_into()?;
    let new_nonce = format!("{}/acme/new_nonce", url).try_into()?;
    let revoke_cert = format!("{}/acme/revoke_cert", url).try_into()?;

    Ok(ApiDirectory {
        key_change,
        meta: None,
        new_authz: None,
        new_order,
        new_account,
        new_nonce,
        revoke_cert,
    })
}

// Some type magic to get future to be static
// even tho we pass config as a reference
pub(super) fn run<A: AcmeServer + 'static>(
    config: &Config,
    server: A,
) -> impl Future<Output = Result<(), Error>> + 'static {
    let server = Arc::new(server);

    let directory = move || {
        let directory = generate_directory(&config.url)?;
        Ok(serde_json::to_vec(&directory)?)
    };
    let directory = match directory() {
        Ok(directory) => directory,
        Err(e) => return future::ready(Err(e)).left_future(),
    };

    routes(directory, server).right_future()
}

async fn routes<A: AcmeServer + 'static>(directory: Vec<u8>, server: Arc<A>) -> Result<(), Error> {
    let directory = warp::path!("acme" / "directory").map(move || directory.clone());

    let new_nonce = warp::path!("acme" / "new_nonce")
        .map(move || server.clone())
        .and_then(nonce);

    let routes = directory.or(new_nonce);

    warp::serve(routes).run(([127, 0, 0, 1], 8000)).await;

    Ok(())
}