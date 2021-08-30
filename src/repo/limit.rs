use async_trait::async_trait;
use std::error::Error;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;
use thiserror::Error;
use tower::Service;
use futures_util::ready;
use futures_util::FutureExt;

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
enum LimitServiceError<E: Error> {
    #[error(transparent)]
    Repo(#[from] E),
    #[error("Reached rate limit")]
    Limited,
}

enum LimitServiceState<R: LimitRepo> {
    Empty,
    Initial(R),
    Working(Pin<Box<dyn Future<Output = Result<((), R), (<R as LimitRepo>::Error, R)>>>>),
    Ready(R),
}

struct LimitService<S, R: LimitRepo> {
    service: S,
    state: LimitServiceState<R>
}

impl<Request, S, R> Service<Request>
    for LimitService<S, R>
where
    R: LimitRepo + 'static,
    S: Service<Request, Error = <R as LimitRepo>::Error>,
{
    type Response = S::Response;
    type Error = LimitServiceError<<R as LimitRepo>::Error>;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send + Sync>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        let state = std::mem::replace(&mut self.state, LimitServiceState::Empty);
        let mut repo = match state {
            LimitServiceState::Empty => unreachable!(),
            LimitServiceState::Working(mut fut) => {
                let res = match fut.poll_unpin(cx) {
                    Poll::Pending => {
                        self.state = LimitServiceState::Working(fut);
                        return Poll::Pending;
                    }
                    Poll::Ready(res) => res,
                };
                return match res {
                    Ok(((), repo)) => {
                        self.state = LimitServiceState::Ready(repo);
                        self.poll_ready(cx)
                    },
                    Err((err, repo)) => {
                        self.state = LimitServiceState::Initial(repo);
                        Poll::Ready(Err(err.into()))
                    }
                };
            }
            LimitServiceState::Ready(repo) => {
                match self.service.poll_ready(cx) {
                    Poll::Pending => {
                        self.state = LimitServiceState::Ready(repo);
                        return Poll::Pending;
                    }
                    Poll::Ready(res) => {
                        self.state = LimitServiceState::Initial(repo);
                        return Poll::Ready(Ok(res?));
                    }
                }
            }
            LimitServiceState::Initial(repo) => repo,
        };

        self.state = LimitServiceState::Working(Box::pin(async move {
            let duration = Duration::from_secs(10);
            let limit = repo.get_limit("hallo", duration).await;
            Ok(((), repo))
        }));

        self.poll_ready(cx)
    }

    fn call(&mut self, req: Request) -> Self::Future {
        todo!()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test() {

    }
}