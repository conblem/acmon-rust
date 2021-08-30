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

enum LimitService<Request, R, S, C, F> {
    Empty(PhantomData<Request>),
    Data { repo: R, service: S, creator: C },
    Working { creator: C, future: F },
}

impl<Request, R, S> LimitService<Request, R, S, (), ()>
where
    R: LimitRepo + 'static,
    S: Service<Request>,
    S::Error: Error,
{
    fn new(repo: R, service: S) -> impl Service<Request> {
        let creator = |mut repo: R, service: S| {
            Box::pin(async move {
                let duration = Duration::from_secs(10);
                let _limit = match repo.get_limit("test", duration).await {
                    Ok(limit) => limit,
                    Err(err) => return (Err(LimitServiceError::Repo(err)), repo, service),
                };

                (Ok(()), repo, service)
            })
        };
        let future = creator(repo, service);

        LimitService::Working { creator, future }
    }
}

impl<Request, R, S, C, F> Service<Request> for LimitService<Request, R, S, C, F>
where
    R: LimitRepo + 'static,
    S: Service<Request>,
    S::Error: Error,
    C: Fn(R, S) -> F,
    F: Future<Output = (Result<(), LimitServiceError<R::Error, S::Error>>, R, S)> + Unpin,
{
    type Response = S::Response;
    type Error = LimitServiceError<<R as LimitRepo>::Error, <S as Service<Request>>::Error>;
    type Future = LimitServiceFuture<R::Error, S::Future>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        let this = std::mem::replace(self, Self::Empty(PhantomData));

        let (creator, mut future) = match this {
            Self::Empty(_) => unreachable!(),
            Self::Data {
                repo,
                service,
                creator,
            } => {
                let future = creator(repo, service);
                *self = Self::Working { creator, future };
                return self.poll_ready(cx);
            }
            Self::Working { creator, future } => (creator, future),
        };

        let (res, repo, service) = match future.poll_unpin(cx) {
            Poll::Pending => {
                *self = Self::Working { creator, future };
                return Poll::Pending;
            }
            Poll::Ready(res) => res,
        };

        *self = Self::Data {
            repo,
            service,
            creator,
        };
        return Poll::Ready(res);
    }

    fn call(&mut self, req: Request) -> Self::Future {
        let service = match self {
            Self::Data { service, .. } => service,
            _ => panic!("Call poll_ready first"),
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

    trait PinServiceTrait<Request> {
        type Response;
        type Error;
        type Future: Future<Output = Result<Self::Response, Self::Error>>;

        fn poll_ready(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>>;
        fn call(self: Pin<&mut Self>, req: Request) -> Self::Future;
    }

    #[test]
    fn test() {}
}
