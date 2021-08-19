use async_trait::async_trait;
use etcd_client::GetOptions;
use std::convert::TryFrom;
use std::error::Error;
use std::future::Future;
use std::marker::PhantomData;
use std::num::TryFromIntError;
use std::time::Duration;
use thiserror::Error;
use tower::Service;
use tracing::{error, instrument};

use super::etcd::{EtcdRequest, EtcdResponse};
use super::time::{SystemTime, Time};

#[async_trait]
trait LimitRepo {
    type Builder: LimitRepoBuilder<Repo = Self>;
    type Error: Error + Send + Sync + 'static;

    async fn get_limit(&mut self, key: &str, range: Duration) -> Result<u32, Self::Error>;
    async fn add_req(&mut self, key: &str) -> Result<(), Self::Error>;
}

trait ToLimitRepoBuilder {
    type Repo: LimitRepo<Builder = Self::Builder>;
    type Builder: LimitRepoBuilder<Repo = Self::Repo>;

    fn builder() -> Self::Builder;
}

impl<R, B> ToLimitRepoBuilder for R
where
    R: LimitRepo<Builder = B>,
    B: LimitRepoBuilder<Repo = R> + Default,
{
    type Repo = R;
    type Builder = B;

    fn builder() -> Self::Builder {
        B::default()
    }
}

trait LimitRepoBuilder {
    type Repo;
    type Error: Error + Send + Sync + 'static;

    fn max_duration(&mut self, range: Duration) -> &mut Self;
    fn build(&mut self) -> Result<Self::Repo, Self::Error>;
}

#[derive(Error, Debug)]
enum EtcdLimitRepoBuilderError {
    #[error("Client not set")]
    ClientNotSet,
}

struct EtcdLimitRepoBuilder<S, T = SystemTime> {
    service: Option<S>,
    max_lease: Duration,
    phantom: PhantomData<T>,
}

impl<S, T> Default for EtcdLimitRepoBuilder<S, T> {
    fn default() -> Self {
        EtcdLimitRepoBuilder {
            service: None,
            max_lease: Duration::default(),
            phantom: PhantomData,
        }
    }
}

impl<S, T> EtcdLimitRepoBuilder<S, T> {
    fn client(&mut self, service: S) -> &mut Self {
        self.service = Some(service);

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

        Ok(EtcdLimitRepo {
            max_lease: self.max_lease,
            service,
            phantom: PhantomData,
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
    Etcd(E),
}

struct EtcdLimitRepo<S, T = SystemTime> {
    max_lease: Duration,
    service: S,
    phantom: PhantomData<T>,
}

#[async_trait]
impl<S, F, E, T> LimitRepo for EtcdLimitRepo<S, T>
where
    S: Service<EtcdRequest, Future = F> + Send + Sync,
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

        let now = T::now();

        let future = now + Duration::from_secs(600);
        let future = future.as_millis();

        let start = now - range;
        let start = start.as_millis();

        let options = GetOptions::new()
            .with_range(format!("limit_{}_{}", key, future))
            .with_count_only();

        let key = format!("limit_{}_{}", key, start).into();
        let req = EtcdRequest::GetWithOptions(key, options);
        let res = match self.service.call(req).await {
            Ok(EtcdResponse::Get(res)) => res.count(),
            Ok(_) => unreachable!(),
            Err(err) => Err(EtcdLimitRepoError::Etcd(err))?
        };

        match u32::try_from(res) {
            Err(err) => Err(EtcdLimitRepoError::CouldNotConvertEtcdCount(res, err)),
            Ok(num) => Ok(num),
        }
    }

    #[instrument(skip(self))]
    async fn add_req(&mut self, key: &str) -> Result<(), Self::Error> {
        // currently no error
        let mut err = Ok(());

        // we retry the request three times incase somebody tries to create a key with the same timestamp
        for i in 0..2 {
            let now = T::now();
            let key = format!("limit_{}_{}", key, now.as_millis());
            let req = EtcdRequest::Put(key.into(), vec![1]);

            err = match self.service.call(req).await {
                Err(err) => {
                    error!("Error {} in Iteration {}", err, i);
                    Err(EtcdLimitRepoError::Etcd(err))
                }
                Ok(_) => return Ok(()),
            };

            tokio::time::sleep(Duration::from_millis(10)).await;
        }

        err
    }
}

#[cfg(test)]
mod tests {
    use etcd_client::proto::PbPutResponse;
    use etcd_client::PutResponse;
    use tower_test::assert_request_eq;
    use tower_test::mock;
    use std::fmt::{Display, Formatter, Debug};
    use std::fmt;
    use tower::ServiceExt;

    use super::super::time::MockTime;
    use super::*;

    struct BoxError(Box<dyn Error + Send + Sync + 'static>);

    impl Display for BoxError {
        fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
            Display::fmt(&self.0, f)
        }
    }

    impl Debug for BoxError {
        fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
            Debug::fmt(&self.0, f)
        }
    }

    impl Error for BoxError {
        fn source(&self) -> Option<&(dyn Error + 'static)> {
            self.0.source()
        }
    }

    impl From<Box<dyn Error + Send + Sync + 'static>> for BoxError {
        fn from(input: Box<dyn Error + Send + Sync + 'static>) -> Self {
            BoxError(input)
        }
    }

    #[tokio::test]
    async fn test() {
        let (service, mut handle) = mock::pair();
        let mut service = service.map_result(|res| res.map_err(BoxError));

        let mut repo: EtcdLimitRepo<_, MockTime> = EtcdLimitRepo::builder()
            .client(service)
            .max_duration(Duration::from_millis(1000))
            .build()
            .expect("build failed");

        let req = repo.get_limit("test", Duration::from_millis(500));
        let req = tokio::spawn(req);

        let expected = EtcdResponse::Put(PutResponse(PbPutResponse {
            header: None,
            prev_kv: None,
        }));

        assert_request_eq!(handle, EtcdRequest::Get(vec![])).send_response(expected);

        req.await;
    }
}
