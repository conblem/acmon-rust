use etcd_client::{Client, Error as EtcdError, GetResponse, PutResponse};
use tower::{service_fn, Service};

use request::{Get, Put};

mod limit;
mod request;

#[derive(Clone)]
struct EtcdService<G, P> {
    get: G,
    put: P,
}

struct GetEtcdService;

impl GetEtcdService {
    fn new(client: Client) -> impl Service<Get, Response = GetResponse, Error = EtcdError> + Clone {
        service_fn(move |req: Get| {
            let mut client = client.clone();

            let Get { key, options } = req;
            async move { client.get(key, options.map(Into::into)).await }
        })
    }
}

struct PutEtcdService;

impl PutEtcdService {
    fn new(client: Client) -> impl Service<Put, Response = PutResponse, Error = EtcdError> + Clone {
        service_fn(move |req| {
            let mut client = client.clone();

            let Put { key, value } = req;
            async move { client.put(key, value, None).await }
        })
    }
}

#[cfg(all(test, feature = "container"))]
mod tests {
    use testcontainers::images::generic::GenericImage;
    use testcontainers::{clients, Container, Docker, Image};
    use tower::ServiceExt;

    use super::*;

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

        let mut get_service = GetEtcdService::new(client.clone());
        let mut put_service = PutEtcdService::new(client);

        let res = (&mut get_service)
            .oneshot(Get::request("test"))
            .await
            .unwrap();
        // etcd is empty so count is 0
        assert_eq!(res.count(), 0);

        // put a new kv into the store so there is no prev key
        let res = (&mut put_service)
            .oneshot(Put::request("test", "is a value"))
            .await
            .unwrap();
        assert!(res.prev_key().is_none());

        // now we get the kv so count should be one
        let res = (&mut get_service)
            .oneshot(Get::request("test"))
            .await
            .unwrap();

        assert_eq!(res.count(), 1);
        let value = &res.kvs()[0];
        assert_eq!(b"test", value.key());
        assert_eq!(b"is a value", value.value());
    }
}
