use etcd_client::Error as EtcdError;
use etcd_client::{Client, GetResponse, KvClient, PutResponse};
use std::future::Future;
use tower::{service_fn, Service};

use request::EtcdRequest;

mod limit;
mod request;

#[derive(Clone, Debug)]
pub(super) enum EtcdResponse {
    Put(PutResponse),
    Get(GetResponse),
}

#[cfg(test)]
impl PartialEq for EtcdResponse {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Put(res), Self::Put(res2)) => res.0 == res2.0,
            (Self::Get(res), Self::Get(res2)) => res.0 == res2.0,
            _ => false,
        }
    }
}

impl From<PutResponse> for EtcdResponse {
    fn from(input: PutResponse) -> Self {
        Self::Put(input)
    }
}

impl From<GetResponse> for EtcdResponse {
    fn from(input: GetResponse) -> Self {
        Self::Get(input)
    }
}

async fn request(mut client: KvClient, req: EtcdRequest) -> Result<EtcdResponse, EtcdError> {
    match req {
        EtcdRequest::Put(key, value, options) => client.put(key, value, None).await.map(Into::into),
        EtcdRequest::Get(key, options) => {
            let options = match options {
                Some(options) => Some(options.into()),
                None => None,
            };
            client.get(key, options).await.map(Into::into)
        }
    }
}

struct EtcdService;

impl EtcdService {
    fn new(
        client: Client,
    ) -> impl Service<EtcdRequest, Future = impl Future<Output = Result<EtcdResponse, EtcdError>>> + Clone
    {
        let client = client.kv_client();

        service_fn(move |req| {
            let client = client.clone();
            request(client, req)
        })
    }
}

#[cfg(all(test, feature = "container"))]
mod tests {
    use etcd_client::proto::{PbPutResponse, PbRangeResponse};
    use etcd_client::Client;
    use testcontainers::images::generic::GenericImage;
    use testcontainers::{clients, Container, Docker, Image};
    use tower::ServiceExt;

    use super::*;
    use super::request::{Put, Get, ToPutRequest, ToGetRequest};

    fn ok<T, E>(input: Result<T, E>) -> T {
        match input {
            Ok(val) => val,
            Err(_) => unreachable!("input is not ok"),
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
            EtcdResponse::Put(_) => unreachable!("res is not a get: {:?}", res),
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
            EtcdResponse::Get(_) => unreachable!("res is not a put: {:?}", res),
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

    #[tokio::test]
    async fn test() {
        let cli = clients::Cli::default();
        let etcd = create_etcd(&cli);
        let client = create_client(&etcd).await;

        let mut service = EtcdService::new(client.clone());
        let service = ok(service.ready().await);

        // etcd is empty so count is 0
        let res = service.call(Get::request("test")).await.unwrap();
        assert_eq!(get_response(res).count(), 0);

        // put a new kv into the store so there is no prev key
        let res = service
            .call(Put::request("test", "is a value"))
            .await
            .unwrap();
        assert!(put_response(res).prev_key().is_none());

        // now we get the kv so count should be one
        let res = service.call(Get::request("test")).await.unwrap();
        let res = get_response(res);
        assert_eq!(res.count(), 1);
        let value = &res.kvs()[0];
        assert_eq!(b"test", value.key());
        assert_eq!(b"is a value", value.value());
    }
}
