use async_trait::async_trait;
use futures_util::ready;
use futures_util::FutureExt;
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

struct LimitService<Request, T> {
    phantom: PhantomData<Request>,
    inner: Pin<Box<T>>,
}

pin_project! {
    #[project_replace = LimitServiceInnerReplace]
    #[project = LimitServiceInnerProj]
    enum LimitServiceInner<R, S, C, F> {
        Empty,
        Data { repo: R, service: S, creator: C },
        Working { creator: C, #[pin] future: F },
    }
}

impl<Request, R, S> LimitService<Request, LimitServiceInner<R, S, (), ()>>
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
        let future = creator(repo, service);
        let inner = LimitServiceInner::Working { creator, future };

        LimitService {
            phantom: PhantomData,
            inner: Box::pin(inner),
        }
    }

    async fn creator(
        mut repo: R,
        service: S,
    ) -> (Result<(), LimitServiceError<R::Error, S::Error>>, R, S)
    {
        let duration = Duration::from_secs(10);
        let _limit = match repo.get_limit("test", duration).await {
            Ok(limit) => limit,
            Err(err) => return (Err(LimitServiceError::Repo(err)), repo, service),
        };

        (Ok(()), repo, service)
    }

}

impl<Request, R, S, C, F> Service<Request> for LimitService<Request, LimitServiceInner<R, S, C, F>>
where
    R: LimitRepo + 'static,
    S: Service<Request>,
    S::Error: Error,
    C: Fn(R, S) -> F + Copy,
    F: Future<Output = (Result<(), LimitServiceError<R::Error, S::Error>>, R, S)>,
{
    type Response = S::Response;
    type Error = LimitServiceError<R::Error, S::Error>;
    type Future = LimitServiceFuture<R::Error, S::Future>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        let inner = &mut self.inner;

        // if inner is working first just poll future using project
        if let LimitServiceInnerProj::Working { future, creator } = inner.as_mut().project() {
            let (res, repo, service) = match future.poll(cx) {
                // future is pending so we just return this
                Poll::Pending => return Poll::Pending,
                Poll::Ready((res, repo, service)) => (res, repo, service),
            };

            // here wo copy the creator
            let creator = *creator;
            inner.as_mut().project_replace(LimitServiceInner::Data {
                creator,
                service,
                repo,
            });
            return Poll::Ready(res);
        }

        let (repo, service, creator) =
            match inner.as_mut().project_replace(LimitServiceInner::Empty) {
                LimitServiceInnerReplace::Data {
                    repo,
                    service,
                    creator,
                } => (repo, service, creator),
                // inner must be data as we catch working above and empty always gets replaced before returning
                _ => unreachable!(),
            };
        let future = creator(repo, service);

        inner
            .as_mut()
            .project_replace(LimitServiceInner::Working { future, creator });

        // recursive call to self to land in above if clause
        self.poll_ready(cx)
    }

    fn call(&mut self, req: Request) -> Self::Future {
        let service = match self.inner.as_mut().project() {
            LimitServiceInnerProj::Data { service, .. } => service,
            _ => panic!("Call ready first"),
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
