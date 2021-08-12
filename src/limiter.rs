use std::time::Duration;

#[derive(Clone)]
struct Config {
    window: Duration,
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
    use std::marker::PhantomData;
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

    struct SlidingMemoryLimiter<S, Request> {
        service: S,
        config: Config,
        phantom: PhantomData<Request>,
        mutex: Arc<Mutex<Option<TimedSizedCache<u64, u16>>>>,
        semaphore: PollSemaphore,
        data: Option<TimedSizedCache<u64, u16>>,
    }

    impl<S, Request> SlidingMemoryLimiter<S, Request>
    where
        S: Service<Request>,
        S::Error: Error,
    {
        fn new(service: S, config: Config) -> Self {
            // calculate size somehow
            let mutex = TimedSizedCache::with_size_and_lifespan(256, config.window.as_secs());
            let semaphore = Arc::new(Semaphore::new(1));

            SlidingMemoryLimiter {
                service,
                config,
                phantom: PhantomData,
                mutex: Arc::new(Mutex::new(Some(mutex))),
                semaphore: PollSemaphore::new(semaphore),
                data: None,
            }
        }

        fn check(
            &mut self,
            data: &mut TimedSizedCache<u64, u16>,
        ) -> Result<(), <Self as Service<Request>>::Error> {
            let sum: u16 = data.value_order().map(|val| val.1).sum();
            if sum < self.config.max {
                return Ok(());
            }

            let current_min = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("Time went backwards")
                .as_secs()
                / 60;

            let bucket = data.cache_get_or_set_with(current_min, || 0);
            *bucket += 1;

            return Err(LimiterError::Ratelimited);
        }
    }

    impl<S: Clone, Request> Clone for SlidingMemoryLimiter<S, Request> {
        fn clone(&self) -> Self {
            SlidingMemoryLimiter {
                service: self.service.clone(),
                config: self.config.clone(),
                phantom: PhantomData,
                mutex: Arc::clone(&self.mutex),
                semaphore: self.semaphore.clone(),
                data: None,
            }
        }
    }

    // if Service::call has not been run but limiter gets dropped we clean up to remove the possibility of a deadlock
    // deadlocks are still possible if you run poll_ready and then leak the limiter
    impl<S, Request> Drop for SlidingMemoryLimiter<S, Request> {
        fn drop(&mut self) {
            if let Some(data) = self.data.take() {
                *self.mutex.lock() = Some(data);
                self.semaphore.add_permits(1);
            }
        }
    }

    impl<S, Request> Service<Request> for SlidingMemoryLimiter<S, Request>
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
                let permit = ready!(self.semaphore.poll_acquire(cx)).expect("semaphore closed");

                let mut guard = self.mutex.lock();
                let mut data = guard.take().expect("cannot be empty");
                drop(guard);
                permit.forget();

                self.check(&mut data)?;

                self.data = Some(data);
            }

            self.service.poll_ready(cx).map_err(LimiterError::from)
        }

        fn call(&mut self, req: Request) -> Self::Future {
            let data = self.data.take().expect("poll ready not called");

            *self.mutex.lock() = Some(data);
            self.semaphore.add_permits(1);

            self.service.call(req).map(map_err)
        }
    }

    fn map_err<T, E: Error>(res: Result<T, E>) -> Result<T, LimiterError<E>> {
        res.map_err(LimiterError::from)
    }
}
