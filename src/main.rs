use acme_core::{AcmeServer, ApiAccount};
use axum::http::header::CONTENT_TYPE;
use axum::http::{HeaderValue, Request, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{middleware, Extension, Json, Router};
use serde::de::{Error as DeError, IntoDeserializer, MapAccess, Visitor};
use serde::{Deserialize, Deserializer};
use std::error::Error;
use std::fmt::Formatter;
use std::marker::PhantomData;

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

    async fn new_account(
        Json(account): Json<SignedRequest<ApiAccount<()>>>,
        Extension(_server): Extension<T>,
    ) -> Response {
        println!("{:?}", account.payload);
        todo!()
    }
}

struct SignedRequest<T> {
    payload: T,
    signature: Vec<u8>,
    protected: Protected,
}

impl<'de, T: Deserialize<'de>> Deserialize<'de> for SignedRequest<T> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct SignedRequestVisitor<T>(PhantomData<T>);
        impl<'de, T: Deserialize<'de>> Visitor<'de> for SignedRequestVisitor<T> {
            type Value = SignedRequest<T>;

            fn expecting(&self, formatter: &mut Formatter) -> std::fmt::Result {
                write!(formatter, "a signed request")
            }

            fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
            where
                A: MapAccess<'de>,
            {
                let mut payload = None;
                let mut signature = None;
                let mut protected = None;

                while let Some((key, val)) = map.next_entry::<&str, &[u8]>()? {
                    let val = base64::decode_config(val, base64::URL_SAFE_NO_PAD)
                        .map_err(A::Error::custom)?;

                    match key {
                        "payload" => payload = Some(T::deserialize(val.into_deserializer())?),
                        "signature" => signature = Some(val),
                        "protected" => {
                            protected = Some(Protected::deserialize(val.into_deserializer())?)
                        }
                        _ => {}
                    }
                }

                match (payload, signature, protected) {
                    (Some(payload), Some(signature), Some(protected)) => Ok(SignedRequest {
                        payload,
                        signature,
                        protected,
                    }),
                    _ => Err(DeError::missing_field("payload, signature, protected")),
                }
            }
        }

        deserializer.deserialize_struct(
            "SignedRequest",
            &["payload", "signature", "protected"],
            SignedRequestVisitor(PhantomData),
        )
    }
}

#[derive(Deserialize)]
struct Protected {}

async fn assert_jose<B>(req: Request<B>, next: Next<B>) -> Result<Response, StatusCode> {
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

#[cfg(test)]
mod tests {
    use crate::AcmeServerServer;
    use acme_core::{AcmeServerBuilder, AcmeServerExt};
    use async_acme::{Directory, HyperAcmeServer};
    use hyper_rustls::HttpsConnectorBuilder;

    #[tokio::test]
    async fn test() {
        let connector = HttpsConnectorBuilder::new()
            .with_webpki_roots()
            .https_only()
            .enable_http1()
            .build();

        let le_staging = HyperAcmeServer::builder()
            .connector(connector.clone())
            .le_staging()
            .build()
            .await
            .unwrap();

        tokio::spawn(async {
            let server = AcmeServerServer { inner: le_staging };
            server.run().await.unwrap();
        });

        let directory = Directory::builder()
            .default()
            .url("http://localhost:3000/directory")
            .build()
            .await
            .unwrap();

        println!("{:?}", directory);
    }
}
