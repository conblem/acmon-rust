use anyhow::Error;
use async_trait::async_trait;
use dto::SignedRequest;
use hyper::client::connect::Connect as HyperConnect;
use serde::Serialize;
use std::fmt::Debug;

mod direct;
mod dto;
mod proxy;

pub(crate) use proxy::ProxyAcmeServer;

#[async_trait]
pub(crate) trait AcmeServerBuilder: Send + Sync + 'static {
    type Server: AcmeServer;
    async fn build(&mut self) -> Result<Self::Server, <Self::Server as AcmeServer>::Error>;
}

#[async_trait]
pub(crate) trait AcmeServer: Send + Sync {
    type Error: Into<Error> + Send;
    type Builder: AcmeServerBuilder<Server = Self>;

    async fn get_nonce(&self) -> Result<String, Self::Error>;

    async fn create_account<S: Serialize + Send>(
        &self,
        req: SignedRequest<(), S>,
    ) -> Result<(), Self::Error>;
}

pub(crate) trait Connect: HyperConnect + Clone + Debug + Send + Sync + 'static {}

impl<C: HyperConnect + Clone + Debug + Send + Sync + 'static> Connect for C {}

pub(crate) trait ToAmceServerBuilder {
    type Server: AcmeServer<Builder = Self::Builder>;
    type Builder: AcmeServerBuilder<Server = Self::Server>;

    fn builder() -> Self::Builder;
}

impl<A: AcmeServer<Builder = B>, B: AcmeServerBuilder<Server = A> + Default> ToAmceServerBuilder
    for A
{
    type Server = A;
    type Builder = B;

    fn builder() -> Self::Builder {
        B::default()
    }
}
