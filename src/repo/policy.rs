use futures_util::ready;
use pin_project_lite::pin_project;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::time::{Duration, Sleep};
use tower::retry::Policy;

#[derive(Clone, Copy)]
pub(super) struct RandomAttempts {
    attempts: usize,
}

impl RandomAttempts {
    pub(super) fn new(attempts: usize) -> Self {
        RandomAttempts { attempts }
    }
}

pin_project! {
    pub(super) struct RandomAttemptsFuture<T> {
        #[pin]
        sleep: Sleep,
        attempts: Option<T>,
    }
}

impl<T> Future for RandomAttemptsFuture<T> {
    type Output = T;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.project();
        ready!(this.sleep.poll(cx));

        let attempts = this.attempts.take().expect("Future called again");
        Poll::Ready(attempts)
    }
}

impl<Request: Clone, Response, E> Policy<Request, Response, E> for RandomAttempts {
    type Future = RandomAttemptsFuture<Self>;

    fn retry(&self, _req: &Request, result: Result<&Response, &E>) -> Option<Self::Future> {
        if result.is_ok() {
            return None;
        }

        if self.attempts == 0 {
            return None;
        }

        let sleep = tokio::time::sleep(Duration::from_millis(100));
        let attempts = RandomAttempts {
            attempts: self.attempts - 1,
        };
        Some(RandomAttemptsFuture {
            sleep,
            attempts: Some(attempts),
        })
    }

    fn clone_request(&self, req: &Request) -> Option<Request> {
        Some(req.clone())
    }
}

#[cfg(test)]
mod tests {
    use std::error::Error;
    use std::sync::Arc;
    use tower::retry::Retry;
    use tower::ServiceExt;
    use tower_test::assert_request_eq;
    use tower_test::mock;

    use super::*;

    // todo: improve this test
    #[tokio::test]
    async fn test() {
        let policy = RandomAttempts::new(1);

        let (service, mut handle) = mock::pair();
        let service = service.map_err(Arc::<dyn Error + Send + Sync>::from);
        let service = Retry::new(policy, service);

        let fut = service.oneshot("hallo".to_string());
        let res = tokio::spawn(fut);

        assert_request_eq!(handle, "hallo".to_string()).send_response("welt");

        assert_eq!(res.await.unwrap().unwrap(), "welt");
    }
}
