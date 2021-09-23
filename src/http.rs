use anyhow::{Error, Result};
use futures_util::FutureExt;
use hyper::{Body, Response};
use std::fmt::{Debug, Formatter};
use std::sync::Arc;
use warp::reject::Reject;
use warp::{Filter, Rejection};

use super::AcmeServer;

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

fn test<A: AcmeServer>(server: A) {
    let server = Arc::new(server);

    warp::path("/acme/new-nonce")
        .map(move || server.clone())
        .and_then(nonce);
}
