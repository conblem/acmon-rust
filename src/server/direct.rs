use anyhow::{anyhow, Error};
use async_trait::async_trait;
use hyper::client::HttpConnector;
use hyper::header::HeaderName;
use hyper::{Body, Client, Request};
use serde::Serialize;
use std::any::Any;

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

pub(crate) struct DirectAcmeServerBuilder<C = HttpConnector> {
    endpoint: Endpoint,
    connector: Option<C>,
}

impl<C> DirectAcmeServerBuilder<C> {
    pub(crate) fn connector<NC>(self, connector: NC) -> DirectAcmeServerBuilder<NC> {
        DirectAcmeServerBuilder {
            endpoint: self.endpoint,
            connector: Some(connector),
        }
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

#[async_trait]
impl<C: Connect> AcmeServerBuilder for DirectAcmeServerBuilder<C> {
    type Server = DirectAcmeServer<C>;

    async fn build(&mut self) -> Result<Self::Server, <Self::Server as AcmeServer>::Error> {
        let replay_nonce_header = HeaderName::from_static(REPLAY_NONCE_HEADER);
        let connector = self
            .connector
            .take()
            .ok_or_else(|| anyhow!("No client configured"))?;
        let client = Client::builder().build(connector);

        let req = Request::get(self.endpoint.to_str()).body(Body::empty())?;
        let _directory = client.request(req).await?;

        Ok(DirectAcmeServer {
            client,
            replay_nonce_header,
        })
    }
}

pub(crate) struct DirectAcmeServer<C = HttpConnector> {
    client: Client<C>,
    replay_nonce_header: HeaderName,
}

impl<C: 'static> DirectAcmeServer<C> {
    pub(crate) fn builder() -> DirectAcmeServerBuilder<C> {
        let mut builder = DirectAcmeServerBuilder {
            endpoint: Endpoint::LetsEncrypt,
            connector: None,
        };

        // set default http connector if generics match
        if let Some(builder) = <dyn Any>::downcast_mut::<DirectAcmeServerBuilder>(&mut builder) {
            builder.connector = Some(HttpConnector::new());
        }

        builder
    }
}

#[async_trait]
impl<C: Connect> AcmeServer for DirectAcmeServer<C> {
    type Error = Error;

    async fn get_nonce(&self) -> Result<String, Self::Error> {
        let req = Request::head("test").body(Body::empty())?;
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
