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
    use futures_util::future::{poll_fn, Map};
    use futures_util::ready;
    use futures_util::FutureExt;
    use parking_lot::Mutex;
    use std::error::Error;
    use std::ops::{Deref, DerefMut};
    use std::sync::Arc;
    use std::task::{Context, Poll};
    use std::time::{SystemTime, UNIX_EPOCH};
    use thiserror::Error;
    use tokio::sync::{OwnedSemaphorePermit, Semaphore};
    use tokio_util::sync::PollSemaphore;
    use tower::Service;

    use super::*;

    #[derive(Clone)]
    struct OwnedMutex<T> {
        mutex: Arc<Mutex<Option<T>>>,
        semaphore: PollSemaphore,
    }

    impl<T> OwnedMutex<T> {
        fn new(val: T) -> Self {
            let semaphore = Arc::new(Semaphore::new(1));
            let mutex = Mutex::new(Some(val));

            OwnedMutex {
                mutex: Arc::new(mutex),
                semaphore: PollSemaphore::new(semaphore),
            }
        }

        fn poll_lock(&mut self, cx: &mut Context<'_>) -> Poll<OwnedMutexGuard<T>> {
            let _permit = ready!(self.semaphore.poll_acquire(cx)).expect("cannot be closed");

            // we take out the t to hint to the compiler that the option is never empty
            let data = self
                .mutex
                .lock()
                .take()
                .expect("cannot be empty if we have permit");

            Poll::Ready(OwnedMutexGuard {
                mutex: self.mutex.clone(),
                _permit,
                data: Some(data),
            })
        }
    }

    struct OwnedMutexGuard<T> {
        mutex: Arc<Mutex<Option<T>>>,
        _permit: OwnedSemaphorePermit,
        data: Option<T>,
    }

    impl<T> Deref for OwnedMutexGuard<T> {
        type Target = T;

        fn deref(&self) -> &Self::Target {
            self.data.as_ref().expect("cannot be empty")
        }
    }

    impl<T> DerefMut for OwnedMutexGuard<T> {
        fn deref_mut(&mut self) -> &mut Self::Target {
            self.data.as_mut().expect("cannot be empty")
        }
    }

    impl<T> Drop for OwnedMutexGuard<T> {
        fn drop(&mut self) {
            debug_assert!(self.data.is_some(), "data is some as we never take it out");
            *self.mutex.lock() = self.data.take();
        }
    }

    #[tokio::test]
    async fn try_lock() {
        let mut lock = OwnedMutex::new(0 as u8);
        let mut guard = poll_fn(|cx| lock.poll_lock(cx)).await;
        *guard += 1;
        drop(guard);

        let guard = poll_fn(|cx| lock.poll_lock(cx)).await;
        assert_eq!(1, *guard)
    }

    #[tokio::test]
    async fn should_deadlock() {
        let mut lock = OwnedMutex::new(0 as u8);
        let mut guard = poll_fn(|cx| lock.poll_lock(cx)).await;
        *guard += 1;

        let guard = poll_fn(|cx| lock.poll_lock(cx));
        tokio::pin!(guard);
        let timeout = tokio::time::sleep(Duration::from_millis(100));
        tokio::pin!(timeout);

        tokio::select! {
            _ = guard => {
                panic!("should be deadlocked");
            }
            _ = timeout => {}
        }
    }

    #[tokio::test]
    async fn try_lock_multiple_threads() {
        let mut lock = OwnedMutex::new(0 as u8);
        let mut guard = poll_fn(|cx| lock.poll_lock(cx)).await;

        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(100)).await;
            *guard += 1;
        });

        let guard = poll_fn(|cx| lock.poll_lock(cx));
        tokio::pin!(guard);
        let sleep = tokio::time::sleep(Duration::from_millis(50));
        tokio::pin!(sleep);

        tokio::select! {
            _ = guard => {
                panic!("guard should not be unlocked")
            }
            _ = sleep => {
            }
        }

        let guard = poll_fn(|cx| lock.poll_lock(cx)).await;
        assert_eq!(1, *guard);
    }

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
        mutex: OwnedMutex<TimedSizedCache<u64, u16>>,
        guard: Option<OwnedMutexGuard<TimedSizedCache<u64, u16>>>,
    }

    impl<S> SlidingMemoryLimiter<S> {
        fn new(service: S, config: Config) -> Self {
            // todo: calculate size and resolution somehow
            let cache = TimedSizedCache::with_size_and_lifespan(256, config.window.as_secs());

            SlidingMemoryLimiter {
                service,
                config,
                mutex: OwnedMutex::new(cache),
                guard: None,
            }
        }
    }

    impl<S: Clone> Clone for SlidingMemoryLimiter<S> {
        fn clone(&self) -> Self {
            SlidingMemoryLimiter {
                service: self.service.clone(),
                config: self.config.clone(),
                mutex: self.mutex.clone(),
                guard: None,
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
            if self.guard.is_none() {
                let guard = ready!(self.mutex.poll_lock(cx));

                let sum: u16 = guard.value_order().map(|val| val.1).sum();
                if sum >= self.config.max {
                    return Poll::Ready(Err(LimiterError::Ratelimited));
                }

                self.guard = Some(guard);
            }

            self.service.poll_ready(cx).map_err(LimiterError::from)
        }

        fn call(&mut self, req: Request) -> Self::Future {
            let mut guard = self.guard.take().expect("poll ready not called");

            // todo: calculate resolution
            let current_min = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("Time went backwards")
                .as_secs()
                / self.config.resolution as u64;
            let bucket = guard.cache_get_or_set_with(current_min, || 0);
            *bucket += 1;

            self.service.call(req).map(map_err)
        }
    }

    fn map_err<T, E: Error>(res: Result<T, E>) -> Result<T, LimiterError<E>> {
        res.map_err(LimiterError::from)
    }
}
