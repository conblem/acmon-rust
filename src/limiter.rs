use std::time::Duration;

#[derive(Clone)]
struct Config {
    window: Duration,
    resolution: u16,
    max: u16,
}

#[cfg(test)]
mod tests {
    use cached::{Cached, TimedSizedCache};
    use futures_util::future::Map;
    use futures_util::ready;
    use futures_util::FutureExt;
    use parking_lot::Mutex;
    use std::error::Error;
    use std::sync::Arc;
    use std::task::{Context, Poll};
    use std::time::{SystemTime, UNIX_EPOCH};
    use thiserror::Error;
    use tokio::sync::Semaphore;
    use tokio_util::sync::PollSemaphore;
    use tower::Service;

    use super::*;

    #[derive(Error, Debug)]
    pub enum LimiterError<E: Error> {
        #[error("Used up Ratelimit")]
        Ratelimited,

        #[error(transparent)]
        Underlying(#[from] E),
    }

    struct SlidingMemoryLimiter<S> {
        service: S,
        config: Config,
        mutex: Arc<Mutex<Option<TimedSizedCache<u64, u16>>>>,
        semaphore: PollSemaphore,
        data: Option<TimedSizedCache<u64, u16>>,
    }

    impl<S> SlidingMemoryLimiter<S> {
        fn new(service: S, config: Config) -> Self {
            // todo: calculate size and resolution somehow
            let mutex = TimedSizedCache::with_size_and_lifespan(256, config.window.as_secs());
            let semaphore = Arc::new(Semaphore::new(1));

            SlidingMemoryLimiter {
                service,
                config,
                mutex: Arc::new(Mutex::new(Some(mutex))),
                semaphore: PollSemaphore::new(semaphore),
                data: None,
            }
        }
    }

    impl<S: Clone> Clone for SlidingMemoryLimiter<S> {
        fn clone(&self) -> Self {
            SlidingMemoryLimiter {
                service: self.service.clone(),
                config: self.config.clone(),
                mutex: Arc::clone(&self.mutex),
                semaphore: self.semaphore.clone(),
                data: None,
            }
        }
    }

    // if Service::call has not been run but limiter gets dropped we clean up to remove the possibility of a deadlock
    // deadlocks are still possible if you run poll_ready and then leak the limiter
    impl<S> Drop for SlidingMemoryLimiter<S> {
        fn drop(&mut self) {
            if let Some(data) = self.data.take() {
                *self.mutex.lock() = Some(data);
                self.semaphore.add_permits(1);
            }
        }
    }

    impl<S, Request> Service<Request> for SlidingMemoryLimiter<S>
    where
        S: Service<Request>,
        S::Error: Error,
    {
        type Response = S::Response;
        type Error = LimiterError<S::Error>;
        type Future = Map<
            S::Future,
            fn(Result<S::Response, S::Error>) -> Result<S::Response, LimiterError<S::Error>>,
        >;

        fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
            if self.data.is_none() {
                let permit = ready!(self.semaphore.poll_acquire(cx)).expect("semaphore is never closed");

                let mut guard = self.mutex.lock();
                let data = guard.take().expect("cannot be empty");

                let sum: u16 = data.value_order().map(|val| val.1).sum();
                if sum >= self.config.max {
                    *guard = Some(data);
                    return Poll::Ready(Err(LimiterError::Ratelimited));
                }

                self.data = Some(data);
                permit.forget();
            }

            self.service.poll_ready(cx).map_err(LimiterError::from)
        }

        fn call(&mut self, req: Request) -> Self::Future {
            let mut data = self.data.take().expect("poll ready not called");

            // todo: calculate resolution
            let current_min = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("Time went backwards")
                .as_secs()
                / self.config.resolution as u64;
            let bucket = data.cache_get_or_set_with(current_min, || 0);
            *bucket += 1;

            *self.mutex.lock() = Some(data);
            self.semaphore.add_permits(1);

            self.service.call(req).map(map_err)
        }
    }

    fn map_err<T, E: Error>(res: Result<T, E>) -> Result<T, LimiterError<E>> {
        res.map_err(LimiterError::from)
    }
}
