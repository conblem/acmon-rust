mod account;
mod etcd;
mod limit;
mod policy;
mod sqlx;

use std::time::{Duration, UNIX_EPOCH};

pub(super) trait Time: Send + Sync {
    fn now(&self) -> Duration;
}

#[derive(Clone, Default)]
pub(super) struct SystemTime;

impl Time for SystemTime {
    fn now(&self) -> Duration {
        UNIX_EPOCH.elapsed().expect("Time not working")
    }
}

#[cfg(test)]
mod tests {
    use tokio::time::sleep;

    use super::*;

    #[tokio::test]
    async fn system_should_return_different_times() {
        let time = SystemTime::default();
        let first = time.now();

        sleep(Duration::from_millis(100)).await;

        let second = time.now();

        assert!(first < second);
    }
}
