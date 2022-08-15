use acme_core::{AcmeServer, ApiDirectory};
use axum::http::header::CONTENT_TYPE;
use axum::http::{HeaderValue, Request, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{middleware, Extension, Json, Router};
use std::error::Error;

#[tokio::main]
async fn main() {
    println!("Hello, world!");
}

struct AcmeServerServer<T> {
    inner: T,
}

impl<T: AcmeServer + Clone + 'static> AcmeServerServer<T> {
    async fn run(self) -> Result<(), Box<dyn Error + Send + Sync + 'static>> {
        let assert_jose = middleware::from_fn(assert_jose);

        let app = Router::new()
            .route("/directory", get(Self::directory))
            .route("/new-account", post(Self::new_account).layer(assert_jose))
            .layer(Extension(self.inner));

        axum::Server::bind(&"0.0.0.0:3000".parse().unwrap())
            .serve(app.into_make_service())
            .await
            .unwrap();

        Err("hallo".into())
    }

    // this is wrong we need to build our custom api directory
    async fn directory(Extension(server): Extension<T>) -> Response {
        Json(server.directory()).into_response()
    }

    async fn new_account(Extension(server): Extension<T>) -> Response {
        todo!()
        server.new_account()
    }
}

async fn assert_jose<B>(mut req: Request<B>, next: Next<B>) -> Result<Response, StatusCode> {
    static APPLICATION_JOSE_JSON: HeaderValue = HeaderValue::from_static("application/jose+json");

    let header = req.headers().get(CONTENT_TYPE);

    let header = match header {
        // todo: improve this error
        None => return Err(StatusCode::BAD_REQUEST),
        Some(header) => header,
    };

    if header != APPLICATION_JOSE_JSON {
        return Err(StatusCode::BAD_REQUEST);
    }

    Ok(next.run(req).await)
}
