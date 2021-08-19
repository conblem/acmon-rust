use etcd_client::Error as EtcdError;
use etcd_client::{Client, GetOptions, GetResponse, KvClient, PutOptions, PutResponse};
use std::future::Future;
use std::marker::PhantomData;
use std::task::{Context, Poll};
use tower::service_fn;
use tower::util::ServiceFn;
use tower::Service;

#[derive(Debug)]
pub(super) enum EtcdRequest {
    Put(Vec<u8>, Vec<u8>),
    PutWithOptions(Vec<u8>, Vec<u8>, PutOptions),
    Get(Vec<u8>),
    GetWithOptions(Vec<u8>, GetOptions),
}

#[derive(Debug)]
pub(super) enum EtcdResponse {
    Put(PutResponse),
    Get(GetResponse),
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

impl EtcdService<(), ()> {
    fn new(
        client: Client,
    ) -> impl Service<
        EtcdRequest,
        Response = EtcdResponse,
        Error = EtcdError,
        Future = impl Future<Output = Result<EtcdResponse, EtcdError>>,
    > {
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
