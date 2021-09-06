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
        repo: Option<R>,
        service: S,
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
        let fut = creator(repo);
        let inner = LimitService {
            phantom: PhantomData,
            repo: None,
            service,
            creator,
            fut,
        };

        Box::pin(inner)
    }

    async fn creator(
        mut repo: R,
    ) -> (Result<(), LimitServiceError<R::Error, S::Error>>, R) {
        let duration = Duration::from_secs(10);
        let _limit = match repo.get_limit("test", duration).await {
            Ok(limit) => limit,
            Err(err) => return (Err(LimitServiceError::Repo(err)), repo),
        };

        (Ok(()), repo)
    }
}

impl<Request, R, S, C, F> Service<Request> for Pin<Box<LimitService<Request, R, S, C, F>>>
where
    R: LimitRepo + 'static,
    S: Service<Request>,
    S::Error: Error,
    C: Fn(R) -> F,
    F: Future<Output = (Result<(), LimitServiceError<R::Error, S::Error>>, R)>,
{
    type Response = S::Response;
    type Error = LimitServiceError<R::Error, S::Error>;
    type Future = LimitServiceFuture<R::Error, S::Future>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        let mut this = self.as_mut().project();

        if this.repo.is_some() {
            let res = match this.service.poll_ready(cx) {
                Poll::Pending => return Poll::Pending,
                Poll::Ready(res) => res.map_err(LimitServiceError::Service),
            };

            let repo = this.repo.take().unwrap();
            let fut = (this.creator)(repo);
            this.fut.set(fut);
            return res.into()
        }

        let (res, repo) = match this.fut.poll(cx) {
            Poll::Pending => return Poll::Pending,
            Poll::Ready((res, repo)) => (res, repo),
        };
        if let Err(err) = res {
            let fut = (this.creator)(repo);
            self.as_mut().project().fut.set(fut);
            return Err(err).into();
        }

        *this.repo = Some(repo);
        self.poll_ready(cx)
    }

    fn call(&mut self, req: Request) -> Self::Future {
        let this = self.as_mut().project();

        let future = this.service.call(req);
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
