use anyhow::{Error, Result};
use futures_util::future::Map;
use futures_util::FutureExt;
use hyper::{Body, Response};
use std::fmt::{Debug, Formatter};
use std::future::Future;
use std::sync::Arc;
use warp::reject::{self, Reject};
use warp::{Filter, Rejection};

use super::AcmeServer;

struct AnyhowRejection(Error);

impl Debug for AnyhowRejection {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl Reject for AnyhowRejection {}

trait Rejector: Sized {
    fn reject(
        self,
    ) -> Map<Self, fn(Result<Response<Body>, Error>) -> Result<Response<Body>, Rejection>>;
}

fn wrap(input: Result<Response<Body>, Error>) -> Result<Response<Body>, Rejection> {
    input.map_err(AnyhowRejection).map_err(reject::custom)
}

impl<T: Future<Output = Result<Response<Body>, Error>>> Rejector for T {
    fn reject(
        self,
    ) -> Map<Self, fn(Result<Response<Body>, Error>) -> Result<Response<Body>, Rejection>> {
        self.map(wrap)
    }
}

fn test<A: AcmeServer>(server: A) {
    let server = Arc::new(server);
    warp::path("/acme/new-nonce")
        .map(move || server.clone())
        .and_then(|server: Arc<A>| {
            async move {
                let nonce = server.get_nonce().await;
                let nonce = nonce.map_err(Into::into)?;

                let body = Response::builder()
                    .header("nonce", nonce)
                    .body(Body::empty())?;
                Ok(body)
            }
            .reject()
        });
}
