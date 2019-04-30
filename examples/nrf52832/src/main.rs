#![no_std]
#![no_main]
#![feature(naked_functions, never_type, asm, const_fn)]

extern crate cortex_m_semihosting;

// pick a panicking behavior
// extern crate panic_halt; // you can put a breakpoint on `rust_begin_unwind` to catch panics
// extern crate panic_abort; // requires nightly
// extern crate panic_itm; // logs messages over ITM; requires ITM support
extern crate panic_semihosting; // logs messages to the host stderr; requires a debugger
use cortex_m_semihosting::{hprintln, hprint};
use cortex_m::peripheral::{NVIC, syst::SystClkSource};
use cortex_m_rt::{entry, exception};
use core::sync::atomic::{AtomicU8, Ordering};

use cortexm_scheduling::{run_threads, get_current_thread, wakeup_thread, yieldk, Thread};
use nrf52832_hal::{
    target,
    gpio::{
        GpioExt,
        Level,
    }
};
use embedded_hal::digital::{
    OutputPin,
    StatefulOutputPin
};

static WAITING_THREAD: AtomicU8 = AtomicU8::new(0xff);

#[entry]
fn main() -> ! {

    let mc = target::CorePeripherals::take().unwrap();
    let pc = target::Peripherals::take().unwrap();

    let parts = pc.P0.split();

    let mut led1 = parts.p0_17.into_push_pull_output(Level::Low);


    let mut syst = mc.SYST;
    // // configures the system timer to trigger a SysTick exception every second
    syst.set_clock_source(SystClkSource::Core);
    // // tick every 0.25 second
    syst.set_reload(0xf423ff);
    // //nvic.enable(Exc::SYS_TICK);

    syst.clear_current();
    syst.enable_counter();
    syst.enable_interrupt();


    let mut stack = [0xDEADBEEF; 1024];
    let thread = Thread::new(&mut stack, move || {
        WAITING_THREAD.store(get_current_thread(), Ordering::Release);

        loop {
            if led1.is_set_high() {
                led1.set_low();
            } else {
                led1.set_high();
            }
            yieldk();
        }
    });

    run_threads(&mut [thread]);
}


#[exception]
fn SysTick() {
    match WAITING_THREAD.load(Ordering::Acquire) {
        0xff => (),
        n => wakeup_thread(n)
    }
}