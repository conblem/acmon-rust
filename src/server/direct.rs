use anyhow::{anyhow, Error};
use async_trait::async_trait;
use hyper::body;
use hyper::header::HeaderName;
use hyper::{Body, Client, Request};
use serde::Serialize;

use super::dto::ApiDirectory;
use super::{AcmeServer, AcmeServerBuilder, Connect, SignedRequest};

const REPLAY_NONCE_HEADER: &str = "replay-nonce";

enum Endpoint {
    LetsEncryptStaging,
    LetsEncrypt,
    Url(String),
}

impl Endpoint {
    fn to_str(&self) -> &str {
        match self {
            Endpoint::LetsEncrypt => "https://acme-v02.api.letsencrypt.org/directory",
            Endpoint::LetsEncryptStaging => {
                "https://acme-staging-v02.api.letsencrypt.org/directory"
            }
            Endpoint::Url(endpoint) => endpoint.as_str(),
        }
    }
}

pub(crate) struct DirectAcmeServerBuilder<C> {
    endpoint: Endpoint,
    connector: Option<C>,
}

impl<C> DirectAcmeServerBuilder<C> {
    pub(crate) fn connector(&mut self, connector: C) -> &mut Self {
        self.connector = Some(connector);

        self
    }

    pub(crate) fn le_staging(&mut self) -> &mut Self {
        self.endpoint = Endpoint::LetsEncryptStaging;

        self
    }

    pub(crate) fn url(&mut self, url: String) -> &mut Self {
        self.endpoint = Endpoint::Url(url);

        self
    }
}

impl<C> Default for DirectAcmeServerBuilder<C> {
    fn default() -> Self {
        DirectAcmeServerBuilder {
            connector: None,
            endpoint: Endpoint::LetsEncrypt,
        }
    }
}

#[async_trait]
impl<C: Connect> AcmeServerBuilder for DirectAcmeServerBuilder<C> {
    type Server = DirectAcmeServer<C>;

    async fn build(&mut self) -> Result<Self::Server, <Self::Server as AcmeServer>::Error> {
        let replay_nonce_header = HeaderName::from_static(REPLAY_NONCE_HEADER);
        let connector = self
            .connector
            .take()
            .ok_or_else(|| anyhow!("No connector configured"))?;

        let client = Client::builder().build(connector);

        let req = Request::get(self.endpoint.to_str()).body(Body::empty())?;
        let mut res = client.request(req).await?;
        let body = body::to_bytes(res.body_mut()).await?;

        let directory = serde_json::from_slice(body.as_ref())?;

        Ok(DirectAcmeServer {
            client,
            replay_nonce_header,
            directory,
        })
    }
}

pub(crate) struct DirectAcmeServer<C> {
    client: Client<C>,
    replay_nonce_header: HeaderName,
    directory: ApiDirectory,
}

#[async_trait]
impl<C: Connect> AcmeServer for DirectAcmeServer<C> {
    type Error = Error;
    type Builder = DirectAcmeServerBuilder<C>;

    async fn get_nonce(&self) -> Result<String, Self::Error> {
        let req = Request::head(&self.directory.new_nonce).body(Body::empty())?;
        let mut res = self.client.request(req).await?;

        let nonce = res
            .headers_mut()
            .remove(&self.replay_nonce_header)
            .ok_or_else(|| anyhow!("No nonce"))?
            .to_str()?
            .to_owned();

        Ok(nonce)
    }

    async fn create_account<S: Serialize + Send>(
        &self,
        req: SignedRequest<(), S>,
    ) -> Result<(), Self::Error> {
        let _req = Request::post("").body(req)?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use anyhow::Result;
    use hyper::client::HttpConnector;
    use hyper_rustls::HttpsConnector;
    use wiremock::{Mock, MockServer, ResponseTemplate};
    use wiremock::matchers::{method, path};
    use std::convert::TryInto;

    use super::*;
    use crate::server::ToAmceServerBuilder;

    fn generate_directory(base: &str) -> Result<ApiDirectory> {
        Ok(ApiDirectory {
            new_nonce: format!("{}/new_nonce", base).try_into()?,
            new_account: format!("{}/new_account", base).try_into()?,
            key_change: format!("{}/key_change", base).try_into()?,
            meta: None,
            new_authz: None,
            new_order: format!("{}/new_order", base).try_into()?,
            revoke_cert: format!("{}/new_order", base).try_into()?,
        })
    }

    #[tokio::test]
    async fn builder_works() -> Result<()> {
        let mock_server = MockServer::start().await;
        let base = mock_server.uri();
        let directory = generate_directory(&base)?;

        Mock::given(method("GET"))
            .and(path("/directory"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&directory))
            .mount(&mock_server)
            .await;

        Mock::given(method("HEAD"))
            .and(path("/new_nonce"))
            .respond_with(ResponseTemplate::new(200).insert_header(REPLAY_NONCE_HEADER, "iamnonce"))
            .mount(&mock_server)
            .await;

        let server = DirectAcmeServer::builder()
            .connector(HttpConnector::new())
            .url(format!("{}/directory", &base))
            .build()
            .await?;

        let actual = server.get_nonce().await?;
        assert_eq!("iamnonce", actual);

        Ok(())
    }
}
