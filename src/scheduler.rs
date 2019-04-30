
use super::{Thread, threads_waiting};


pub fn run_threads(threads: &mut [Thread]) -> ! {
    loop {
        for thread in threads.iter_mut() {
            let _switch_reason = thread.switch_to();
        }

        cortex_m::interrupt::free(|_cs| {
            if !threads_waiting() {
                // Critical section prevents an interrupt between is_empty check and wfi.
                // Interrupts will still wake the processor though.
                cortex_m::asm::wfi();
            }
        });
    }
}