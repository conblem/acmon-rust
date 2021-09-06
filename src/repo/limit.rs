use async_trait::async_trait;
use futures_util::future::poll_fn;
use futures_util::{ready, FutureExt};
use pin_project_lite::pin_project;
use std::error::Error;
use std::future::Future;
use std::marker::PhantomData;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;
use thiserror::Error;
use tower::Service;

#[async_trait]
pub(super) trait LimitRepo {
    type Builder: LimitRepoBuilder<Repo = Self>;
    type Error: Error + Send + Sync + 'static;

    async fn get_limit(&mut self, key: &str, range: Duration) -> Result<u32, Self::Error>;
    async fn add_req(&mut self, key: &str) -> Result<(), Self::Error>;
}

pub(super) trait ToLimitRepoBuilder {
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

pub(super) trait LimitRepoBuilder {
    type Repo;
    type Error: Error + Send + Sync + 'static;

    fn max_duration(&mut self, range: Duration) -> &mut Self;
    fn build(&mut self) -> Result<Self::Repo, Self::Error>;
}

#[derive(Debug, Error)]
enum LimitServiceError<R: Error, S: Error> {
    #[error(transparent)]
    Repo(R),

    #[error(transparent)]
    Service(S),

    #[error("Reached rate limit")]
    Limited,
}

pin_project! {
    struct LimitService<Request, R, S, C, F> {
        phantom: PhantomData<Request>,
        data: Option<(R, S)>,
        creator: C,
        #[pin] fut: F,
    }
}

impl<Request, R, S> LimitService<Request, R, S, (), ()>
where
    R: LimitRepo + 'static,
    S: Service<Request>,
    S::Error: Error,
{
    fn new(
        repo: R,
        service: S,
    ) -> impl Service<Request, Response = S::Response, Error = LimitServiceError<R::Error, S::Error>>
    {
        let creator = Self::creator;
        let fut = creator(repo, service);
        let inner = LimitService {
            phantom: PhantomData,
            data: None,
            creator,
            fut,
        };

        Box::pin(inner)
    }

    async fn creator(
        mut repo: R,
        mut service: S,
    ) -> (Result<(), LimitServiceError<R::Error, S::Error>>, R, S) {
        let duration = Duration::from_secs(10);
        let _limit = match repo.get_limit("test", duration).await {
            Ok(limit) => limit,
            Err(err) => return (Err(LimitServiceError::Repo(err)), repo, service),
        };

        // call the poll_ready of the inner service
        let inner = poll_fn(|cx| service.poll_ready(cx));
        if let Err(err) = inner.await {
            return (Err(LimitServiceError::Service(err)), repo, service);
        }

        (Ok(()), repo, service)
    }
}

impl<Request, R, S, C, F> Service<Request> for Pin<Box<LimitService<Request, R, S, C, F>>>
where
    R: LimitRepo + 'static,
    S: Service<Request>,
    S::Error: Error,
    C: Fn(R, S) -> F,
    F: Future<Output = (Result<(), LimitServiceError<R::Error, S::Error>>, R, S)>,
{
    type Response = S::Response;
    type Error = LimitServiceError<R::Error, S::Error>;
    type Future = LimitServiceFuture<R::Error, S::Future>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        let mut this = self.as_mut().project();

        if let Some((repo, service)) = this.data.take() {
            let fut = (this.creator)(repo, service);
            this.fut.set(fut)
        }

        let (res, repo, service) = match this.fut.poll(cx) {
            Poll::Pending => return Poll::Pending,
            Poll::Ready((res, repo, service)) => (res, repo, service),
        };
        *this.data = Some((repo, service));
        Poll::Ready(res)
    }

    fn call(&mut self, req: Request) -> Self::Future {
        let this = self.as_mut().project();
        let service = match this.data {
            Some((_, service)) => service,
            None => panic!("Call ready first"),
        };

        let future = service.call(req);
        LimitServiceFuture {
            phantom: PhantomData,
            future,
        }
    }
}

pin_project! {
    struct LimitServiceFuture<R, F> {
        phantom: PhantomData<R>,
        #[pin]
        future: F,
    }
}

impl<R, T, E, F> Future for LimitServiceFuture<R, F>
where
    F: Future<Output = Result<T, E>>,
    R: Error,
    E: Error,
{
    type Output = Result<T, LimitServiceError<R, E>>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.project();
        let res = ready!(this.future.poll(cx));
        let res = res.map_err(|err| LimitServiceError::Service(err));

        Poll::Ready(res)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test() {}
}
