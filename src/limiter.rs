use std::time::Duration;

struct Config {
    window: Duration,
    max: u16,
}

#[cfg(test)]
mod tests {
    use cached::{TimedSizedCache, Cached};
    use std::time::{SystemTime, UNIX_EPOCH};
    use tokio::sync::Semaphore;
    use std::sync::Arc;
    use std::task::{Context, Poll};
    use tower::Service;
    use tokio_util::sync::PollSemaphore;
    use parking_lot::Mutex;
    use futures_util::ready;

    struct SlidingMemoryLimiter<S> {
        service: S,
        mutex: Arc<Mutex<Option<TimedSizedCache<u64, u16>>>>,
        semaphore: PollSemaphore,
        data: Option<TimedSizedCache<u64, u16>>,
    }

    impl<S: Clone> Clone for SlidingMemoryLimiter<S> {
        fn clone(&self) -> Self {
            SlidingMemoryLimiter {
                service: self.service.clone(),
                mutex: Arc::clone(&self.mutex),
                semaphore: self.semaphore.clone(),
                data: None,
            }
        }
    }

    impl <S> SlidingMemoryLimiter<S> {
        fn new(service: S) -> Self {
            let mutex = TimedSizedCache::with_size_and_lifespan(100, 60);
            let semaphore = Arc::new(Semaphore::new(1));

            SlidingMemoryLimiter {
                service,
                mutex: Arc::new(Mutex::new(Some(mutex))),
                semaphore: PollSemaphore::new(semaphore),
                data: None,
            }
        }

        fn check(&mut self, data: &mut TimedSizedCache<u64, u16>) {
            let current_min = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("Time went backwards")
                .as_secs()
                / 60;

            let bucket = data.cache_get_or_set_with(current_min, || 0);
            *bucket += 1;
        }
    }

    impl<S, Request> Service<Request> for SlidingMemoryLimiter<S>
    where
        S: Service<Request>,
    {
        type Response = S::Response;
        type Error = S::Error;
        type Future = S::Future;

        fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
            if self.data.is_none() {
                let permit = ready!(self.semaphore.poll_acquire(cx));
                permit.expect("semaphore closed").forget();

                let mut guard = self.mutex.lock();
                let mut data = guard.take().expect("cannot be empty");

                self.check(&mut data);

                self.data = Some(data);
            }
            self.service.poll_ready(cx)
        }

        fn call(&mut self, req: Request) -> Self::Future {
            let mut data = self.data.take().expect("poll ready not called");

            *self.mutex.lock() = Some(data);
            self.semaphore.add_permits(1);

            self.service.call(req)
        }
    }
}
