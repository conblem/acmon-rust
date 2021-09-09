use async_trait::async_trait;
use futures_util::ready;
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
    struct LimitService<Request, S, C, F> {
        phantom: PhantomData<Request>,
        outer_ready: bool,
        service: S,
        creator: C,
        #[pin] fut: F,
    }
}

impl<Request, S> LimitService<Request, S, (), ()>
where
    S: Service<Request>,
    S::Error: Error,
{
    fn new<R: LimitRepo + 'static>(
        repo: R,
        service: S,
    ) -> impl Service<Request, Response = S::Response, Error = LimitServiceError<R::Error, S::Error>>
    {
        let creator = Self::creator;
        let fut = creator(repo);

        Box::pin(LimitService {
            phantom: PhantomData,
            outer_ready: false,
            service,
            creator,
            fut,
        })
    }

    async fn creator<R: LimitRepo + 'static>(
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

impl<Request, R, S, C, F> Service<Request> for Pin<Box<LimitService<Request, S, C, F>>>
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

        if *this.outer_ready {
            let res = ready!(this.service.poll_ready(cx));

            *this.outer_ready = false;
            return res.map_err(LimitServiceError::Service).into()
        }

        let (res, repo) = ready!(this.fut.poll(cx));
        if let Err(err) = res {
            let fut = (this.creator)(repo);
            self.as_mut().project().fut.set(fut);
            return Err(err).into();
        }

        *this.outer_ready = true;
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
        let res = res.map_err(LimitServiceError::Service);

        Poll::Ready(res)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test() {}
}
