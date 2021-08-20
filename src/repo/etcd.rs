use etcd_client::Error as EtcdError;
use etcd_client::{Client, GetOptions, GetResponse, KvClient, PutOptions, PutResponse};
use std::future::Future;
use std::marker::PhantomData;
use std::task::{Context, Poll};
use tower::service_fn;
use tower::util::ServiceFn;
use tower::Service;

#[derive(Clone, Debug)]
pub(super) enum EtcdRequest {
    Put(Vec<u8>, Vec<u8>),
    PutWithOptions(Vec<u8>, Vec<u8>, PutOptions),
    Get(Vec<u8>),
    GetWithOptions(Vec<u8>, GetOptions),
}

// we cant compare options so we only implement this in tests
#[cfg(test)]
impl PartialEq for EtcdRequest {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (EtcdRequest::Put(key, val), EtcdRequest::Put(key2, val2)) => {
                key == key2 && val == val2
            }
            (
                EtcdRequest::PutWithOptions(val, key, _),
                EtcdRequest::PutWithOptions(val2, key2, _),
            ) => val == val2 && key == key2,
            (EtcdRequest::Get(key), EtcdRequest::Get(key2)) => key == key2,
            (EtcdRequest::GetWithOptions(key, _), EtcdRequest::GetWithOptions(key2, _)) => {
                key == key2
            }
            _ => false,
        }
    }
}

#[derive(Clone, Debug)]
pub(super) enum EtcdResponse {
    Put(PutResponse),
    Get(GetResponse),
}

#[cfg(test)]
impl PartialEq for EtcdResponse {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (EtcdResponse::Put(res), EtcdResponse::Put(res2)) => res.0 == res2.0,
            (EtcdResponse::Get(res), EtcdResponse::Get(res2)) => res.0 == res2.0,
            _ => false,
        }
    }
}

impl From<PutResponse> for EtcdResponse {
    fn from(input: PutResponse) -> Self {
        EtcdResponse::Put(input)
    }
}

impl From<GetResponse> for EtcdResponse {
    fn from(input: GetResponse) -> Self {
        EtcdResponse::Get(input)
    }
}

async fn request(mut client: KvClient, req: EtcdRequest) -> Result<EtcdResponse, EtcdError> {
    match req {
        EtcdRequest::Put(key, value) => client.put(key, value, None).await.map(Into::into),
        EtcdRequest::PutWithOptions(key, value, options) => {
            let options = options.into();
            client.put(key, value, options).await.map(Into::into)
        }
        EtcdRequest::Get(key) => client.get(key, None).await.map(Into::into),
        EtcdRequest::GetWithOptions(key, options) => {
            let options = options.into();
            client.get(key, options).await.map(Into::into)
        }
    }
}

struct EtcdService<R, F>(ServiceFn<R>, PhantomData<F>);

impl<R: Clone, F> Clone for EtcdService<R, F> {
    fn clone(&self) -> Self {
        EtcdService(self.0.clone(), PhantomData)
    }
}

impl EtcdService<(), ()> {
    fn new(
        client: Client,
    ) -> impl Service<
        EtcdRequest,
        Response = EtcdResponse,
        Error = EtcdError,
        Future = impl Future<Output = Result<EtcdResponse, EtcdError>>,
    > + Clone {
        let client = client.kv_client();

        let service = service_fn(move |req| {
            let client = client.clone();
            request(client, req)
        });

        EtcdService(service, PhantomData)
    }
}

impl<R, F> Service<EtcdRequest> for EtcdService<R, F>
where
    R: FnMut(EtcdRequest) -> F,
    F: Future<Output = Result<EtcdResponse, EtcdError>>,
{
    type Response = EtcdResponse;
    type Error = EtcdError;
    type Future = F;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.0.poll_ready(cx)
    }

    fn call(&mut self, req: EtcdRequest) -> Self::Future {
        self.0.call(req)
    }
}
