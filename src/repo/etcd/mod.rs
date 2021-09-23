use etcd_client::{Client, Error as EtcdError, GetResponse, PutResponse};
use tower::{service_fn, Service};
use etcd_client::GetOptions as EtcdGetOptions;

mod limit;

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

#[derive(Debug, PartialEq, Default)]
pub(crate) struct GetOptions {
    range_end: Vec<u8>,
    count_only: bool,
}

impl From<GetOptions> for EtcdGetOptions {
    fn from(options: GetOptions) -> Self {
        let result = EtcdGetOptions::new();
        let result = if options.count_only {
            result.with_count_only()
        } else {
            result
        };

        result.with_range(options.range_end)
    }
}

#[derive(Debug, PartialEq)]
pub(crate) struct Get {
    pub(crate) key: Vec<u8>,
    pub(crate) options: Option<GetOptions>,
}

impl Get {
    pub(crate) fn request<K: Into<Vec<u8>>>(key: K) -> Self {
        Get {
            key: key.into(),
            options: None,
        }
    }

    pub(crate) fn with_range<R: Into<Vec<u8>>>(&mut self, range_end: R) -> &mut Self {
        let options = self.options.get_or_insert_with(Default::default);
        options.range_end = range_end.into();

        self
    }

    pub(crate) fn with_count_only(&mut self) -> &mut Self {
        let options = self.options.get_or_insert_with(Default::default);
        options.count_only = true;

        self
    }

    // todo: rethink this api
    pub(crate) fn build(&mut self) -> Self {
        std::mem::replace(
            self,
            Get {
                key: Vec::new(),
                options: None,
            },
        )
    }
}

#[derive(Debug, PartialEq)]
pub(crate) struct Put {
    pub(crate) key: Vec<u8>,
    pub(crate) value: Vec<u8>,
}

impl Put {
    pub(crate) fn request<K: Into<Vec<u8>>, V: Into<Vec<u8>>>(key: K, value: V) -> Self {
        Put {
            key: key.into(),
            value: value.into(),
        }
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
