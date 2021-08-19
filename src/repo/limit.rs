use async_trait::async_trait;
use etcd_client::{Client, GetOptions};
use std::convert::TryFrom;
use std::error::Error;
use std::num::TryFromIntError;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use thiserror::Error;

#[async_trait]
trait LimitRepo {
    type Builder: LimitRepoBuilder<Repo = Self>;
    type Error: Error + Send + Sync + 'static;

    fn builder() -> Self::Builder {
        <Self::Builder as Default>::default()
    }

    async fn get_limit(&mut self, key: &str, range: Duration) -> Result<u32, Self::Error>;
}

trait LimitRepoBuilder: Default {
    type Repo: LimitRepo;
    type Error: Error + Send + Sync + 'static;

    fn max_duration(&mut self, range: Duration) -> &mut Self;
    fn build(&mut self) -> Result<Self::Repo, Self::Error>;
}

#[derive(Error, Debug)]
enum EtcdLimitRepoBuilderError {
    #[error("Client not set")]
    ClientNotSet,
}

#[derive(Default)]
struct EtcdLimitRepoBuilder {
    max_lease: Duration,
    client: Option<Client>,
}

impl EtcdLimitRepoBuilder {
    fn client(&mut self, client: Client) -> &mut Self {
        self.client = Some(client);

        self
    }
}

impl LimitRepoBuilder for EtcdLimitRepoBuilder {
    type Repo = EtcdLimitRepo;
    type Error = EtcdLimitRepoBuilderError;

    fn max_duration(&mut self, range: Duration) -> &mut Self {
        self.max_lease = range;

        self
    }

    fn build(&mut self) -> Result<Self::Repo, Self::Error> {
        let client = self
            .client
            .take()
            .ok_or(EtcdLimitRepoBuilderError::ClientNotSet)?;

        Ok(EtcdLimitRepo {
            max_lease: self.max_lease,
            client,
        })
    }
}

#[derive(Error, Debug)]
enum EtcdLimitRepoError {
    #[error("Could not convert Etcd's {0} to an u32: {1}")]
    CouldNotConvertEtcdCount(i64, TryFromIntError),
}

struct EtcdLimitRepo {
    max_lease: Duration,
    client: Client,
}

#[async_trait]
impl LimitRepo for EtcdLimitRepo {
    type Builder = EtcdLimitRepoBuilder;
    type Error = EtcdLimitRepoError;

    async fn get_limit(&mut self, key: &str, range: Duration) -> Result<u32, Self::Error> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("Time not working");

        let future = now + Duration::from_secs(600);
        let future = future.as_secs();

        let start = now - range;
        let start = start.as_secs();

        let options = GetOptions::new()
            .with_range(format!("limit_{}_{}", key, future))
            .with_count_only();

        // todo: fix excpects in this function
        let res = self
            .client
            .get(format!("limit_{}_{}", key, start), Some(options))
            .await
            .expect("etcd is stoopid")
            .count();

        match u32::try_from(res) {
            Err(err) => Err(EtcdLimitRepoError::CouldNotConvertEtcdCount(res, err)),
            Ok(num) => Ok(num),
        }
    }
}
