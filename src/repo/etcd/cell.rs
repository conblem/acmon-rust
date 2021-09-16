use std::cell::Cell;
use std::task::{Context, Poll};
use tower::Service;

pub(super) struct CloneOnceService<'a, T>(Cell<Option<&'a mut T>>);

// implements clone which can only be called once
// does not implement sync which is not a problem as futures dont need sync
impl<'a, T> CloneOnceService<'a, T> {
    pub(super) fn new(inner: &'a mut T) -> Self {
        CloneOnceService(Cell::new(Some(inner)))
    }
}

impl<'a, Request, T> Service<Request> for CloneOnceService<'a, T>
where
    T: Service<Request>,
{
    type Response = T::Response;
    type Error = T::Error;
    type Future = T::Future;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        let inner = self
            .0
            .get_mut()
            .as_deref_mut()
            .expect("Service can only be cloned once");

        inner.poll_ready(cx)
    }

    fn call(&mut self, req: Request) -> Self::Future {
        let inner = self
            .0
            .get_mut()
            .as_deref_mut()
            .expect("Service can only be cloned once");

        inner.call(req)
    }
}

impl<'a, T> Clone for CloneOnceService<'a, T> {
    fn clone(&self) -> Self {
        let inner = self.0.replace(None);
        CloneOnceService(Cell::new(inner))
    }
}

#[cfg(test)]
mod tests {
    use async_trait::async_trait;
    use futures_util::future::Ready;
    use tower::retry::Policy;
    use tower::{service_fn, ServiceBuilder, ServiceExt};

    use super::*;

    #[derive(Clone)]
    struct TestPolicy(usize);

    impl Policy<(), &'static str, &'static str> for TestPolicy {
        type Future = Ready<Self>;

        fn retry(
            &self,
            _: &(),
            result: Result<&&'static str, &&'static str>,
        ) -> Option<Self::Future> {
            match result {
                Ok(_) => None,
                Err(_) => {
                    if self.0 > 0 {
                        Some(futures_util::future::ready(Self(self.0 - 1)))
                    } else {
                        None
                    }
                }
            }
        }

        fn clone_request(&self, _: &()) -> Option<()> {
            Some(())
        }
    }

    #[async_trait]
    trait Test {
        async fn hallo(&mut self) -> Result<&'static str, &'static str>;
    }

    struct TestImpl<T>(T, usize);

    #[async_trait]
    impl<T> Test for TestImpl<T>
    where
        T: Service<(), Response = &'static str, Error = &'static str> + Send,
        T::Future: Send,
    {
        async fn hallo(&mut self) -> Result<&'static str, &'static str> {
            ServiceBuilder::new()
                .retry(TestPolicy(self.1))
                .service(CloneOnceService::new(&mut self.0))
                .oneshot(())
                .await
        }
    }

    fn test_type<T: Send>(_inner: T) {}

    #[tokio::test]
    async fn test() {
        let service =
            service_fn(|_: ()| async { Ok("hallo") as Result<&'static str, &'static str> });
        let mut service = TestImpl(service, 10);

        test_type(service.hallo());
        let actual = service.hallo().await.unwrap();
        assert_eq!(actual, "hallo");
    }
}
