use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

use argentum_domain::RunId;
use tracing::debug;

#[derive(Clone, Default)]
pub struct RunCancellation {
    cancelled: Arc<AtomicBool>,
}

impl RunCancellation {
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }
}

#[derive(Debug, Default)]
pub struct Scheduler;

impl Scheduler {
    pub fn note_run_cancelled(&self, run_id: RunId) {
        debug!(run_id = %run_id, "run cancellation recorded");
    }
}
