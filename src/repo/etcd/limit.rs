use async_trait::async_trait;
use etcd_client::{GetResponse, PutResponse};
use std::convert::TryFrom;
use std::error::Error;
use std::num::TryFromIntError;
use std::time::Duration;
use thiserror::Error;
use tower::util::MapRequest;
use tower::{Service, ServiceExt};
use tracing::instrument;

use super::super::limit::{LimitRepo, LimitRepoBuilder, ToLimitRepoBuilder};
use super::super::time::{SystemTime, Time};
use super::{Get, Put};

struct EtcdLimitRepoBuilder<R, U, T = SystemTime> {
    read_service: Option<R>,
    update_service: Option<U>,
    max_lease: Duration,
    time: Option<T>,
}

impl<R, U, T> Default for EtcdLimitRepoBuilder<R, U, T> {
    fn default() -> Self {
        EtcdLimitRepoBuilder {
            read_service: None,
            update_service: None,
            max_lease: Duration::default(),
            time: None,
        }
    }
}

impl<R, U, T> EtcdLimitRepoBuilder<R, U, T> {
    fn read_service(&mut self, read_service: R) -> &mut Self {
        self.read_service = Some(read_service);
        self
    }

    fn update_service(&mut self, update_service: U) -> &mut Self {
        self.update_service = Some(update_service);
        self
    }

    fn time(&mut self, time: T) -> &mut Self {
        self.time = Some(time);
        self
    }
}

impl<R, U, T> LimitRepoBuilder for EtcdLimitRepoBuilder<R, U, T>
where
    R: Service<Get> + Clone,
    U: Service<Put> + Clone,
    T: Time,
{
    type Repo = EtcdLimitRepo<R, U, T>;

    fn max_duration(&mut self, range: Duration) -> &mut Self {
        self.max_lease = range;
        self
    }

    fn build(&mut self) -> Self::Repo {
        let read_service = self.read_service.take().expect("read_service not set");

        let update_service = self.update_service.take().expect("update_service not set");
        let update_service = update_service.map_request(
            (|key_and_time| {
                let (key, time) = key_and_time;
                let now = time.now();
                let key = format!("limit_{}_{}", key, now.as_millis());

                Put::request(key, vec![1])
            }) as fn((&str, &T)) -> Put,
        );

        let time = self.time.take().expect("Time not set");

        let max_lease = self.max_lease;
        if max_lease <= Duration::from_secs(0) {
            panic!("Lease cannot be {:?}", max_lease);
        }

        EtcdLimitRepo {
            read_service,
            update_service,
            max_lease,
            time,
        }
    }
}

#[derive(Error, Debug)]
enum EtcdLimitRepoError<E: Error + Send + Sync + 'static> {
    #[error("Could not convert Etcd's {0} to an u32: {1}")]
    CouldNotConvertEtcdCount(i64, TryFromIntError),

    #[error("Duration {0} is longer than max {1}")]
    RangeLongerThanMax(u128, u128),

    #[error(transparent)]
    Etcd(#[from] E),
}

struct EtcdLimitRepo<R, U, T = SystemTime> {
    read_service: R,
    update_service: MapRequest<U, fn((&str, &T)) -> Put>,
    max_lease: Duration,
    time: T,
}

#[async_trait]
impl<R, U, E, T> LimitRepo for EtcdLimitRepo<R, U, T>
where
    R: Service<Get, Response = GetResponse, Error = E> + Clone + Send + Sync,
    R::Future: Send,
    U: Service<Put, Response = PutResponse, Error = E> + Clone + Send + Sync,
    U::Future: Send,
    E: Error + Send + Sync + 'static,
    T: Time,
{
    type Builder = EtcdLimitRepoBuilder<R, U, T>;
    type Error = EtcdLimitRepoError<E>;

    #[instrument(skip(self), fields(range = %range.as_millis()))]
    async fn get_limit(&self, key: &str, range: Duration) -> Result<u32, Self::Error> {
        if range > self.max_lease {
            return Err(EtcdLimitRepoError::RangeLongerThanMax(
                range.as_millis(),
                self.max_lease.as_millis(),
            ));
        }

        let now = self.time.now();

        // we add 60 seconds in the future if the time of another node is in the future
        let future = now + Duration::from_secs(60);
        let future = future.as_millis();
        let start = now - range;

        let req = Get::request(format!("limit_{}_{}", key, start.as_millis()))
            .with_range(format!("limit_{}_{}", key, future))
            .with_count_only()
            .build();

        let res = self.read_service.clone().oneshot(req).await?.count();

        match u32::try_from(res) {
            Err(err) => Err(EtcdLimitRepoError::CouldNotConvertEtcdCount(res, err)),
            Ok(num) => Ok(num),
        }
    }

    #[instrument(skip(self))]
    async fn add_req(&self, key: &str) -> Result<(), Self::Error> {
        let time = &self.time;

        self.update_service.clone().oneshot((key, time)).await?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use etcd_client::proto::PbRangeResponse;
    use etcd_client::GetResponse;
    use mockall::mock;
    use std::sync::Arc;
    use std::time::{SystemTime as StdSystemTime, UNIX_EPOCH};
    use tower::util::MapErr;
    use tower::ServiceExt;
    use tower_test::mock::Handle;
    use tower_test::{assert_request_eq, mock};

    use super::*;

    mock! {
        Time {}
        impl Time for Time {
            fn now(&self) -> Duration;
        }
    }

    type Mock<Req, Res> = mock::Mock<Req, Res>;
    type MockServiceError = Arc<dyn Error + Send + Sync>;
    type MockServiceMapper<Req, Res> =
        fn(<Mock<Req, Res> as Service<Req>>::Error) -> MockServiceError;
    type MockService<Req, Res> = MapErr<Mock<Req, Res>, MockServiceMapper<Req, Res>>;

    fn create_mock_service<Req, Res>() -> (MockService<Req, Res>, Handle<Req, Res>) {
        let (service, handle) = mock::pair();
        let service = service.map_err(MockServiceError::from as MockServiceMapper<Req, Res>);

        (service, handle)
    }

    #[tokio::test]
    async fn test() {
        let (read_service, mut read_handle) = create_mock_service();
        let (update_service, _) = create_mock_service();

        let now = StdSystemTime::now().duration_since(UNIX_EPOCH).unwrap();
        let mut time = MockTime::new();
        time.expect_now().returning(move || now);

        let repo = EtcdLimitRepo::builder()
            .read_service(read_service)
            .update_service(update_service)
            .time(time)
            .max_duration(Duration::from_secs(1000))
            .build();

        let req =
            tokio::spawn(async move { repo.get_limit("test", Duration::from_secs(500)).await });

        let res = GetResponse(PbRangeResponse {
            header: None,
            kvs: vec![],
            more: false,
            count: 200,
        });

        let start = now - Duration::from_secs(500);
        let future = now + Duration::from_secs(60);

        let key = format!("limit_test_{}", start.as_millis());
        let expected = Get::request(key)
            .with_range(format!("limit_test_{}", future.as_millis()))
            .with_count_only()
            .build();
        assert_request_eq!(read_handle, expected).send_response(res);

        let actual = req.await.unwrap().unwrap();
        assert_eq!(actual, 200);
    }

    #[should_panic]
    #[test]
    fn should_panic_on_default_lease() {
        let (read_service, _) = create_mock_service::<_, GetResponse>();
        let (update_service, _) = create_mock_service::<_, PutResponse>();

        EtcdLimitRepoBuilder::default()
            .read_service(read_service)
            .update_service(update_service)
            .time(SystemTime::default())
            .build();
    }
}
