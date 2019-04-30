use core::sync::atomic::{AtomicBool, Ordering};
use core::cell::UnsafeCell;
use core::ops::{Deref, DerefMut};
use super::{ThreadList, get_current_thread, wakeup_threads, yieldk};

pub struct Mutex<T: ?Sized> {
    locked: AtomicBool,
    waiting_threads: ThreadList,
    data: UnsafeCell<T>,
}

unsafe impl<T: ?Sized + Send> Send for Mutex<T> {}
unsafe impl<T: ?Sized + Send> Sync for Mutex<T> {}

pub struct MutexGuard<'a, T: ?Sized + 'a> {
    lock: &'a Mutex<T>,
}

impl<T> Mutex<T> {
    pub fn new(t: T) -> Self {
        Self {
            locked: AtomicBool::new(false),
            waiting_threads: ThreadList::new(),
            data: UnsafeCell::new(t),
        }
    }
}

impl<T: ?Sized> Mutex<T> {
    pub fn lock(&self) -> MutexGuard<T> {
        while self.locked.compare_and_swap(false, true, Ordering::Acquire) == false {
            self.waiting_threads.add(get_current_thread());
            yieldk();
        }
        MutexGuard { lock: &self }
    }

    pub fn try_lock(&self) -> Option<MutexGuard<T>> {
        if self.locked.compare_and_swap(false, true, Ordering::Acquire) {
            Some(MutexGuard { lock: &self })
        } else {
            None
        }
    }
}


impl<T: ?Sized> Deref for MutexGuard<'_, T> {
    type Target = T;

    fn deref(&self) -> &T {
        unsafe { &*self.lock.data.get() }
    }
}

impl<T: ?Sized> DerefMut for MutexGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        unsafe { &mut *self.lock.data.get() }
    }
}

impl<T: ?Sized> Drop for MutexGuard<'_, T> {
    #[inline]
    fn drop(&mut self) {
        self.lock.locked.store(false, Ordering::Release);
        // If wakeup_threads gets reordered before the store then we may
        // get spurious wakeups. The Mutex handles this by spin_waiting
        // for the lock and re-scheduling the wakeup.
        wakeup_threads(self.lock.waiting_threads.get_all());
    }
}