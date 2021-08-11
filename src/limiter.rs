use async_trait::async_trait;

#[async_trait]
trait Limiter {
    type Key;

    async fn request(&self, key: Self::Key);
}

#[cfg(test)]
mod tests {
    use cached::{Cached, TimedSizedCache};
    use parking_lot::Mutex;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;

    struct MemoryLimiter {
        data: Mutex<TimedSizedCache<String, u8>>
    }

    #[async_trait]
    impl Limiter for MemoryLimiter {
        type Key = String;

        async fn request(&self, key: Self::Key) {
            let mut data = self.data.lock();
            let current_min = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("Time went backwards")
                .as_secs() / 60;
            let value = data.cache_get_or_set_with(key, || 0);
            *value += 1;

            let amount: u8 = data.value_order().map(|val| val.1).sum();
        }
    }
}