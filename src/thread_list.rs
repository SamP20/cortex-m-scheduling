use core::sync::atomic::{AtomicU32, Ordering};

pub struct ThreadList {
    threads: AtomicU32
}


impl ThreadList {
    pub const fn new() -> Self {
        ThreadList{ threads: AtomicU32::new(0) }
    }

    pub fn add_all(&self, ids: u32) {
        // SeqCst to prevent reordering, especially across yield points
        self.threads.fetch_or(ids, Ordering::SeqCst);
    }

    pub fn add(&self, task: u8) {
        self.threads.fetch_or(1 << task, Ordering::SeqCst);
    }

    pub fn remove(&self, task: u8) -> bool {
        let bit = 1 << task;
        // Mask the bit regardless. If it was set then return true
        // Removing wakeups doesn't need to be as strict as adding them
        self.threads.fetch_and(!bit, Ordering::AcqRel) & bit == bit
    }

    pub fn is_empty(&self) -> bool {
        self.threads.load(Ordering::Acquire) > 0
    }

    pub fn get_all(&self) -> u32 {
        self.threads.swap(0, Ordering::Acquire)
    }

    pub fn get_iter(&self) -> ThreadListIter {
        ThreadListIter::new(self.threads.swap(0, Ordering::Acquire))
    }

}

pub struct ThreadListIter {
    threads: u32,
    current: u8
}

impl ThreadListIter {
    pub fn new(threads: u32) -> Self {
        ThreadListIter{ threads, current: 0 }
    }
}

impl Iterator for ThreadListIter {
    type Item = u8;

    fn next(&mut self) -> Option<Self::Item> {
        while self.current < 32 {
            let current_bit = 1<<self.current;
            if self.threads & current_bit == current_bit {
                let ret = self.current;
                self.current += 1;
                return Some(ret);
            }
            self.current += 1;
        }

        None
    }
}