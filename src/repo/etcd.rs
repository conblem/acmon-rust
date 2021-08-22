use etcd_client::Error as EtcdError;
use etcd_client::{Client, GetOptions, GetResponse, KvClient, PutOptions, PutResponse};
use std::future::Future;
use std::marker::PhantomData;
use std::task::{Context, Poll};
use tower::util::ServiceFn;
use tower::{service_fn, Service};

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
    ) -> impl Service<EtcdRequest, Future = impl Future<Output = Result<EtcdResponse, EtcdError>>> + Clone
    {
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

#[cfg(test)]
mod tests {
    use etcd_client::proto::{PbPutResponse, PbRangeResponse};
    use etcd_client::Client;
    use testcontainers::images::generic::GenericImage;
    use testcontainers::{clients, Container, Docker, Image};
    use tower::ServiceExt;

    use super::*;

    fn ok<T, E>(input: Result<T, E>) -> T {
        match input {
            Ok(val) => val,
            Err(_) => unreachable!(),
        }
    }

    #[should_panic]
    #[test]
    fn test_ok() {
        let res = Err("test") as Result<&'static str, &'static str>;
        ok(res);
    }

    fn get_response(res: EtcdResponse) -> GetResponse {
        match res {
            EtcdResponse::Get(res) => res,
            EtcdResponse::Put(_) => unreachable!(),
        }
    }

    #[should_panic]
    #[test]
    fn test_get_response() {
        let res = EtcdResponse::Put(PutResponse(PbPutResponse {
            header: None,
            prev_kv: None,
        }));
        get_response(res);
    }

    fn put_response(res: EtcdResponse) -> PutResponse {
        match res {
            EtcdResponse::Put(res) => res,
            EtcdResponse::Get(_) => unreachable!(),
        }
    }

    #[should_panic]
    #[test]
    fn test_put_response() {
        let res = EtcdResponse::Get(GetResponse(PbRangeResponse {
            header: None,
            kvs: vec![],
            more: false,
            count: 0,
        }));
        put_response(res);
    }

    fn create_etcd(cli: &clients::Cli) -> Container<clients::Cli, GenericImage> {
        let image = GenericImage::new("quay.io/coreos/etcd:v3.5.0")
            .with_args(vec![
                "--listen-client-urls=http://0.0.0.0:2379".into(),
                "--advertise-client-urls=http://0.0.0.0:2379".into(),
            ])
            .with_entrypoint("/usr/local/bin/etcd");

        cli.run(image)
    }

    async fn create_client(etcd: &Container<'_, clients::Cli, GenericImage>) -> Client {
        let port = etcd.get_host_port(2379).unwrap();
        Client::connect([format!("http://localhost:{}", port)], None)
            .await
            .unwrap()
    }

    #[cfg(feature = "container")]
    #[tokio::test]
    async fn test() {
        let cli = clients::Cli::default();
        let etcd = create_etcd(&cli);
        let client = create_client(&etcd).await;

        let mut service = EtcdService::new(client.clone());
        let service = ok(service.ready().await);

        let res = service.call(EtcdRequest::Get("test".into())).await.unwrap();
        let res = get_response(res);

        assert_eq!(res.count(), 0);
    }
}
