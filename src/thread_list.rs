use core::sync::atomic::{AtomicU32, Ordering};
use super::{
    ThreadID,
    thread::schedule_all
};

pub struct ThreadList {
    threads: AtomicU32
}

impl ThreadList {
    pub const fn new() -> Self {
        ThreadList{ threads: AtomicU32::new(0) }
    }

    pub fn add(&self, thread: ThreadID) {
        self.threads.fetch_or(1 << thread.raw(), Ordering::SeqCst);
    }

    pub fn schedule_all(&self) {
        schedule_all(self.threads.swap(0, Ordering::Acquire))
    }

    // Private API
    pub(crate) fn remove(&self, thread: ThreadID) -> bool {
        let bit = 1 << thread.raw();
        // Mask the bit regardless. If it was set then return true
        // Removing wakeups doesn't need to be as strict as adding them
        self.threads.fetch_and(!bit, Ordering::AcqRel) & bit == bit
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.threads.load(Ordering::Acquire) > 0
    }

    pub(crate) fn add_all(&self, ids: u32) {
        // SeqCst to prevent reordering, especially across yield points
        self.threads.fetch_or(ids, Ordering::SeqCst);
    }
}