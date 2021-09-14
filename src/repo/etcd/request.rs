use etcd_client::GetOptions as EtcdGetOptions;
use std::mem;

#[derive(PartialEq, Debug)]
pub(crate) enum EtcdRequest {
    Put(Vec<u8>, Vec<u8>, Option<()>),
    Get(Vec<u8>, Option<GetOptions>),
}

pub(crate) trait ToPutRequest {
    fn request<K: Into<Vec<u8>>, V: Into<Vec<u8>>>(&mut self, key: K, val: V) -> EtcdRequest;
}

pub(crate) trait ToGetRequest {
    fn request<K: Into<Vec<u8>>>(&mut self, key: K) -> EtcdRequest;
}

#[derive(Default)]
pub(crate) struct Put;

impl ToPutRequest for Put {
    fn request<K: Into<Vec<u8>>, V: Into<Vec<u8>>>(&mut self, key: K, val: V) -> EtcdRequest {
        EtcdRequest::Put(key.into(), val.into(), None)
    }
}

#[derive(Default)]
pub(crate) struct Get;

impl ToGetRequest for Get {
    fn request<K: Into<Vec<u8>>>(&mut self, key: K) -> EtcdRequest {
        EtcdRequest::Get(key.into(), None)
    }
}

#[derive(PartialEq, Default, Debug)]
pub(crate) struct GetOptions {
    range_end: Vec<u8>,
    count_only: bool,
}

// todo: write test for this using docker
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

impl GetOptions {
    pub(crate) fn with_range<R: Into<Vec<u8>>>(&mut self, range_end: R) -> &mut Self {
        self.range_end = range_end.into();

        self
    }

    pub(crate) fn with_count_only(&mut self) -> &mut Self {
        self.count_only = true;

        self
    }
}

impl ToGetRequest for GetOptions {
    fn request<K: Into<Vec<u8>>>(&mut self, key: K) -> EtcdRequest {
        let options = mem::take(self);
        EtcdRequest::Get(key.into(), Some(options))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[should_panic]
    fn test_unwrap_put() {
        let request = EtcdRequest::Get(Vec::new(), None);
        unwrap_put(request);
    }

    fn unwrap_put(request: EtcdRequest) -> (Vec<u8>, Vec<u8>, Option<()>) {
        match request {
            EtcdRequest::Put(key, val, option) => (key, val, option),
            _ => unreachable!(),
        }
    }

    #[test]
    #[should_panic]
    fn test_unwrap_get() {
        let request = EtcdRequest::Put(Vec::new(), Vec::new(), None);
        unwrap_get(request);
    }

    fn unwrap_get(request: EtcdRequest) -> (Vec<u8>, Option<GetOptions>) {
        match request {
            EtcdRequest::Get(key, option) => (key, option),
            _ => unreachable!(),
        }
    }

    #[test]
    fn put() {
        let request = Put::default().request("hallo", "welt");
        let (key, val, options) = unwrap_put(request);

        assert_eq!(key, b"hallo");
        assert_eq!(val, b"welt");
        assert!(options.is_none());
    }

    #[test]
    fn get() {
        let request = Get::default().request("hallo");
        let (key, options) = unwrap_get(request);

        assert_eq!(key, b"hallo");
        assert!(options.is_none());
    }

    #[test]
    fn get_with_options() {
        let mut expected = GetOptions::default();
        expected.with_count_only().with_range("hallo_100");

        let request = GetOptions::default()
            .with_count_only()
            .with_range("hallo_100")
            .request("hallo_1");
        let (key, options) = unwrap_get(request);
        let options = options.unwrap();

        assert_eq!(key, b"hallo_1");
        assert_eq!(options.count_only, true);
        assert_eq!(options.range_end, b"hallo_100");
    }
}
