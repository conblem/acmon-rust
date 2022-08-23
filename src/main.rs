use acme_core::{AcmeServer, ApiAccount, ApiDirectory};
use axum::http::header::{HeaderName, CACHE_CONTROL, CONTENT_TYPE};
use axum::http::uri::InvalidUri;
use axum::http::{HeaderValue, Request, StatusCode};
use axum::middleware::Next;
use axum::response::{AppendHeaders, IntoResponse, Response};
use axum::routing::{get, head, post};
use axum::{middleware, Extension, Json, Router};
use serde::de::{DeserializeOwned, Error as DeError, MapAccess, Visitor};
use serde::{Deserialize, Deserializer};
use std::error::Error;
use std::fmt::Formatter;
use std::marker::PhantomData;
use std::sync::Arc;

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
        let directory_extension = Self::create_directory("127.0.0.1:3000").unwrap();

        let app = Router::new()
            .route(
                "/directory",
                get(Self::directory).layer(directory_extension),
            )
            .route("/new-account", post(Self::new_account).layer(assert_jose))
            .route("/new-nonce", head(Self::new_nonce))
            .layer(Extension(self.inner));

        axum::Server::bind(&"127.0.0.1:3000".parse().unwrap())
            .serve(app.into_make_service())
            .await
            .unwrap();

        Err("hallo".into())
    }

    fn create_directory(addr: &'static str) -> Result<Extension<Arc<ApiDirectory>>, InvalidUri> {
        let directory = ApiDirectory {
            new_nonce: format!("http://{}/new-nonce", addr).try_into()?,
            new_account: format!("http://{}/new-account", addr).try_into()?,
            new_order: format!("http://{}/new-order", addr).try_into()?,
            new_authz: None,
            revoke_cert: format!("http://{}/revoke-cert", addr).try_into()?,
            key_change: format!("http://{}/key-change", addr).try_into()?,
            meta: None,
        };

        Ok(Extension(Arc::new(directory)))
    }

    // this is wrong we need to build our custom api directory
    async fn directory(Extension(directory): Extension<Arc<ApiDirectory>>) -> Response {
        Json(&*directory).into_response()
    }

    async fn new_account(
        Json(account): Json<SignedRequest<ApiAccount<()>>>,
        Extension(server): Extension<T>,
    ) -> StatusCode {
        println!("{:?}", account.payload);
        StatusCode::OK
    }

    async fn new_nonce(Extension(server): Extension<T>) -> impl IntoResponse {
        // todo: remove unwrap;
        let nonce = server.new_nonce().await.unwrap();
        AppendHeaders([
            (CACHE_CONTROL, HeaderValue::from_static("no-store")),
            (
                HeaderName::from_static("replay-nonce"),
                HeaderValue::from_str(&nonce).unwrap(),
            ),
        ])
    }
}

struct SignedRequest<T> {
    payload: T,
    signature: Vec<u8>,
}

impl<'de, T: DeserializeOwned> Deserialize<'de> for SignedRequest<T> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct SignedRequestVisitor<T>(PhantomData<T>);
        impl<'de, T: DeserializeOwned> Visitor<'de> for SignedRequestVisitor<T> {
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

                while let Some((key, val)) = map.next_entry::<&str, &[u8]>()? {
                    let val = base64::decode_config(val, base64::URL_SAFE_NO_PAD)
                        .map_err(A::Error::custom)?;

                    match key {
                        "payload" => {
                            payload = Some(serde_json::from_slice(&*val).map_err(A::Error::custom)?)
                        }
                        "signature" => signature = Some(val),
                        _ => {}
                    }
                }

                match (payload, signature) {
                    (Some(payload), Some(signature)) => Ok(SignedRequest { payload, signature }),
                    _ => Err(DeError::missing_field("payload, signature, protected")),
                }
            }
        }

        deserializer.deserialize_struct(
            "SignedRequest",
            &["payload", "signature"],
            SignedRequestVisitor(PhantomData),
        )
    }
}

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
            .https_or_http()
            .enable_http1()
            .build();

        let le_staging = HyperAcmeServer::builder()
            .connector(connector.clone())
            .le_staging()
            .build()
            .await
            .unwrap();

        let mut local_server = HyperAcmeServer::builder();
        local_server.connector(connector);

        tokio::spawn(async {
            let server = AcmeServerServer { inner: le_staging };
            server.run().await.unwrap();
        });

        let directory = Directory::builder()
            .server(local_server)
            .url("http://localhost:3000/directory")
            .build()
            .await
            .unwrap();

        let account = directory.new_account("test@mail.com").await.unwrap();

        println!("{:?}", account);
    }
}
