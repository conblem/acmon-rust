use etcd_client::GetOptions as EtcdGetOptions;

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

#[cfg(test)]
mod tests {
    use super::*;
}
