use anyhow::Error;
use async_trait::async_trait;
use hyper_rustls::HttpsConnector;
use serde::Serialize;
use std::marker::PhantomData;
use std::ops::{Deref, DerefMut};
use warp::hyper::client::HttpConnector;

use super::direct::{DirectAcmeServer, DirectAcmeServerBuilder};
use super::{AcmeServer, AcmeServerBuilder, SignedRequest};

pub(crate) struct ProxyAcmeServerBuilder<B = DirectAcmeServerBuilder<HttpsConnector<HttpConnector>>>(
    B,
);

#[async_trait]
impl<A: AcmeServer<Builder = B>, B: AcmeServerBuilder<Server = A>> AcmeServerBuilder
    for ProxyAcmeServerBuilder<B>
{
    type Server = ProxyAcmeServer<A, B>;

    async fn build(&mut self) -> Result<Self::Server, <Self::Server as AcmeServer>::Error> {
        let inner = self.0.build().await;
        let inner = inner.map_err(Into::into)?;

        Ok(ProxyAcmeServer {
            inner,
            builder: PhantomData,
        })
    }
}

impl<A: AcmeServer<Builder = B>, B: AcmeServerBuilder<Server = A> + Default> Default
    for ProxyAcmeServerBuilder<B>
{
    fn default() -> Self {
        ProxyAcmeServerBuilder(B::default())
    }
}

impl<B> Deref for ProxyAcmeServerBuilder<B> {
    type Target = B;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<B> DerefMut for ProxyAcmeServerBuilder<B> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

pub(crate) struct ProxyAcmeServer<
    A = DirectAcmeServer<HttpsConnector<HttpConnector>>,
    B = DirectAcmeServerBuilder<HttpsConnector<HttpConnector>>,
> {
    inner: A,
    builder: PhantomData<B>,
}

impl ProxyAcmeServer {
    pub(crate) fn builder() -> ProxyAcmeServerBuilder {
        let mut builder = DirectAcmeServerBuilder::default();
        builder.connector(HttpsConnector::with_webpki_roots());

        ProxyAcmeServerBuilder(builder)
    }
}

#[async_trait]
impl<A: AcmeServer<Builder = B>, B: AcmeServerBuilder<Server = A>> AcmeServer
    for ProxyAcmeServer<A, B>
{
    type Error = Error;
    type Builder = ProxyAcmeServerBuilder<B>;

    async fn get_nonce(&self) -> Result<String, Self::Error> {
        let nonce = self.inner.get_nonce().await;
        let nonce = nonce.map_err(Into::into)?;

        Ok(nonce)
    }

    async fn create_account<S: Serialize + Send>(
        &self,
        req: SignedRequest<(), S>,
    ) -> Result<(), Self::Error> {
        let account = self.inner.create_account(req).await;
        let account = account.map_err(Into::into)?;

        Ok(account)
    }
}
