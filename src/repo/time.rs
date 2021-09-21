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
