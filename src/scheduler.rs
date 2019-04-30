use core::sync::atomic::{AtomicU8, Ordering};
use super::{Thread, ThreadList};

const INVALID_THREAD_ID: u8 = 0xff;

static CURRENT_THREAD: AtomicU8 = AtomicU8::new(INVALID_THREAD_ID);
// Bitfield of threads that have been woken up by interrupts
static THREAD_WAKEUPS: ThreadList = ThreadList::new();


pub fn run_threads(threads: &mut [Thread]) -> ! {
    if get_current_thread() != INVALID_THREAD_ID {
        panic!("Cannot call run_threads from within a thread")
    }

    wakeup_threads((1 << threads.len()) - 1);

    loop {
        for (index, thread) in threads.iter_mut().enumerate() {
            if THREAD_WAKEUPS.remove(index as u8) {
                CURRENT_THREAD.store(index as u8, Ordering::Release);
                let _switch_reason = thread.switch_to();
            }
        }
        CURRENT_THREAD.store(INVALID_THREAD_ID, Ordering::Release);

        cortex_m::interrupt::free(|_cs| {
            if THREAD_WAKEUPS.is_empty() {
                // Critical section prevents an interrupt between is_empty check and wfi.
                // Interrupts will still wake the processor though.
                cortex_m::asm::wfi();
            }
        }) 
    }
}



/// Gets the currently running thread.
pub fn get_current_thread() -> u8 {
    CURRENT_THREAD.load(Ordering::Acquire)
}

/// Schedules a batch of threads to run
pub fn wakeup_threads(ids: u32) {
    THREAD_WAKEUPS.add_all(ids);
}

/// Schedules the thread to be run. Call this from interrupts to resume work.
pub fn wakeup_thread(id: u8) {
    THREAD_WAKEUPS.add(id);
}