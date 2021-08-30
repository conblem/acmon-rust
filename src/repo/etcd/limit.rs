use async_trait::async_trait;
use etcd_client::GetOptions;
use std::convert::TryFrom;
use std::error::Error;
use std::future::Future;
use std::num::TryFromIntError;
use std::time::{Duration, SystemTime};
use thiserror::Error;
use tower::retry::Retry;
use tower::{Service, ServiceExt};
use tracing::instrument;

use super::super::limit::{LimitRepo, LimitRepoBuilder, ToLimitRepoBuilder};
use super::super::policy::RandomAttempts;
use super::super::time::Time;
use super::{EtcdRequest, EtcdResponse};

#[derive(Error, Debug)]
enum EtcdLimitRepoBuilderError {
    #[error("Client not set")]
    ClientNotSet,

    #[error("Time not set")]
    TimeNotSet,
}

struct EtcdLimitRepoBuilder<S, T = SystemTime> {
    service: Option<S>,
    max_lease: Duration,
    time: Option<T>,
}

impl<S, T> Default for EtcdLimitRepoBuilder<S, T> {
    fn default() -> Self {
        EtcdLimitRepoBuilder {
            service: None,
            max_lease: Duration::default(),
            time: None,
        }
    }
}

impl<S, T> EtcdLimitRepoBuilder<S, T> {
    fn client(&mut self, service: S) -> &mut Self {
        self.service = Some(service);

        self
    }

    fn time(&mut self, time: T) -> &mut Self {
        self.time = Some(time);

        self
    }
}

impl<S, T> LimitRepoBuilder for EtcdLimitRepoBuilder<S, T> {
    type Repo = EtcdLimitRepo<S, T>;
    type Error = EtcdLimitRepoBuilderError;

    fn max_duration(&mut self, range: Duration) -> &mut Self {
        self.max_lease = range;

        self
    }

    fn build(&mut self) -> Result<Self::Repo, Self::Error> {
        let service = self
            .service
            .take()
            .ok_or(EtcdLimitRepoBuilderError::ClientNotSet)?;

        let time = self
            .time
            .take()
            .ok_or(EtcdLimitRepoBuilderError::TimeNotSet)?;

        Ok(EtcdLimitRepo {
            max_lease: self.max_lease,
            service,
            time,
            attempts: RandomAttempts::new(3),
        })
    }
}

#[derive(Error, Debug)]
enum EtcdLimitRepoError<E: Error + Send + Sync + 'static> {
    #[error("Could not convert Etcd's {0} to an u32: {1}")]
    CouldNotConvertEtcdCount(i64, TryFromIntError),

    #[error("Duration {0} is longer than max {0}")]
    RangeLongerThanMax(u128, u128),

    #[error(transparent)]
    Etcd(#[from] E),
}

struct EtcdLimitRepo<S, T = SystemTime> {
    max_lease: Duration,
    service: S,
    time: T,
    attempts: RandomAttempts,
}

#[async_trait]
impl<S, F, E, T> LimitRepo for EtcdLimitRepo<S, T>
where
    S: Service<EtcdRequest, Response = EtcdResponse, Error = E, Future = F>
        + Clone
        + Send
        + Sync
        + 'static,
    F: Future<Output = Result<EtcdResponse, E>> + Send + Sync,
    E: Error + Send + Sync + 'static,
    T: Time,
{
    type Builder = EtcdLimitRepoBuilder<S, T>;
    type Error = EtcdLimitRepoError<E>;

    #[instrument(skip(self), fields(range = %range.as_millis()))]
    async fn get_limit(&mut self, key: &str, range: Duration) -> Result<u32, Self::Error> {
        if range > self.max_lease {
            return Err(EtcdLimitRepoError::RangeLongerThanMax(
                range.as_millis(),
                self.max_lease.as_millis(),
            ));
        }

        let now = self.time.now();

        let future = now + Duration::from_secs(600);
        let future = future.as_millis();

        let start = now - range;
        let start = start.as_millis();

        let options = GetOptions::new()
            .with_range(format!("limit_{}_{}", key, future))
            .with_count_only();

        let key = format!("limit_{}_{}", key, start).into();
        let req = EtcdRequest::GetWithOptions(key, options);
        let res = match (&mut self.service).oneshot(req).await? {
            EtcdResponse::Get(res) => res.count(),
            _ => unreachable!(),
        };

        match u32::try_from(res) {
            Err(err) => Err(EtcdLimitRepoError::CouldNotConvertEtcdCount(res, err)),
            Ok(num) => Ok(num),
        }
    }

    #[instrument(skip(self))]
    async fn add_req(&mut self, key: &str) -> Result<(), Self::Error> {
        // we use map_request so we can recalculate the request everytime to get the current time
        let service = self.service.clone().map_request(|()| {
            let now = self.time.now();
            let key = format!("limit_{}_{}", key, now.as_millis());
            EtcdRequest::Put(key.into(), vec![1])
        });
        Retry::new(self.attempts.clone(), service)
            .oneshot(())
            .await?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use etcd_client::proto::PbRangeResponse;
    use etcd_client::GetResponse;
    use std::sync::Arc;
    use std::time::{SystemTime as StdSystemTime, UNIX_EPOCH};
    use tower::ServiceExt;
    use tower_test::{assert_request_eq, mock};

    use super::super::super::time::MockTime;
    use super::*;

    #[tokio::test]
    async fn test() {
        let (service, mut handle) = mock::pair();
        let service = service.map_result(|res| res.map_err(Arc::<dyn Error + Send + Sync>::from));

        let now = StdSystemTime::now().duration_since(UNIX_EPOCH).unwrap();
        let mut time = MockTime::new();
        time.expect_now().returning(move || now);

        let mut repo: EtcdLimitRepo<_, MockTime> = EtcdLimitRepo::builder()
            .client(service)
            .time(time)
            .max_duration(Duration::from_millis(1000))
            .build()
            .expect("build failed");

        let req =
            tokio::spawn(async move { repo.get_limit("test", Duration::from_millis(500)).await });

        let res = EtcdResponse::Get(GetResponse(PbRangeResponse {
            header: None,
            kvs: vec![],
            more: false,
            count: 200,
        }));

        let duration = now - Duration::from_millis(500);
        let key = format!("limit_test_{}", duration.as_millis()).into();
        let expected = EtcdRequest::GetWithOptions(key, GetOptions::new());
        assert_request_eq!(handle, expected).send_response(res);

        let actual = req.await.unwrap().unwrap();
        assert_eq!(actual, 200);
    }
}
