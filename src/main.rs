#![no_std]
#![no_main]
#![feature(naked_functions, never_type, asm)]

extern crate nrf52832_pac;
extern crate cortex_m_semihosting;

// pick a panicking behavior
// extern crate panic_halt; // you can put a breakpoint on `rust_begin_unwind` to catch panics
// extern crate panic_abort; // requires nightly
// extern crate panic_itm; // logs messages over ITM; requires ITM support
extern crate panic_semihosting; // logs messages to the host stderr; requires a debugger
use cortex_m_semihosting::{hprintln, hprint};
use cortex_m::peripheral::{NVIC, syst::SystClkSource};
use cortex_m_rt::{entry, exception};

use core::{mem, ptr, slice};

#[entry]
fn main() -> ! {
    let _ = hprintln!("started!").unwrap();

    let _cp = cortex_m::Peripherals::take().unwrap();

    // // configures the system timer to trigger a SysTick exception every second
    // syst.set_clock_source(SystClkSource::Core);
    // // tick every 1 second
    // syst.set_reload(16_777_215);
    // //nvic.enable(Exc::SYS_TICK);

    // syst.clear_current();
    // syst.enable_counter();
    // syst.enable_interrupt();

    let t = Test {
        num1: 0x1122_3344_5566_7788,
        num2: 69,
        msg: "Hello world",
    };

    let mut stack = [0xDEADBEEF; 1024];
    let mut task = Task::new(&mut stack, move || {
        let _ = hprint!(t.msg); // TODO: Find out why hprintln breaks with "{:?}"
    });

    task.switch_to();

    let _ = hprintln!("And we're back!").unwrap();

    loop {
        
    }
}


#[derive(Debug)]
struct Test {
    num1: u64,
    num2: u32,
    msg: &'static str,
}


struct Task<'a> {
    stack: &'a mut [usize],
    task_regs: [usize; 8],
    sp: *const usize,
}

impl<'a> Task<'a> {
    fn new<F>(stack: &'a mut [usize], func: F) -> Self
        where F: FnOnce()
    {
        let mut top = stack.len();

        let data_ptr = (&func as *const F) as *const usize;
        let mut dsize = mem::size_of::<F>()/4; // Assume alignment is at least 4

        if dsize > 4 {
            let remaining = dsize - 4; // 4 u32s will be stored in r0-r3
            top -= remaining; // Remainder stored in stack

            unsafe {
                let src_ptr = data_ptr.offset(4);
                let dst_ptr = stack.as_mut_ptr().offset(top as isize) as usize;
                let dst_aligned = dst_ptr & (!(mem::align_of::<F>()/4 - 1));
                top -= dst_ptr - dst_aligned;
                ptr::copy_nonoverlapping(src_ptr, dst_aligned as *mut usize, remaining);
            }

            dsize = 4;
        }

        stack[top - 1] = 1 << 24; // xPSR
        stack[top - 2] = run_closure::<F> as usize;
        stack[top - 3] = 0xFFFFFFFD; // LR
        stack[top - 4] = 0xCCCCCCCC; // R12
        stack[top - 5] = 0x33333333; // R3
        stack[top - 6] = 0x22222222; // R2
        stack[top - 7] = 0x11111111; // R1
        stack[top - 8] = 0x00000000; // R0

        let sp = unsafe { stack.as_mut_ptr().offset(top as isize - 8) };

        unsafe {
            ptr::copy_nonoverlapping(data_ptr, sp, dsize);
        }

        let task_regs = [
            0x77777777, // R7
            0x66666666, // R6
            0x55555555, // R5
            0x44444444, // R4
            0xBBBBBBBB, // R11
            0xAAAAAAAA, // R10
            0x99999999, // R9
            0x88888888, // R8
        ];

        core::mem::forget(func);

        Task { stack, task_regs, sp }
    }

    fn switch_to(&mut self) {
        unsafe {
            self.sp = switch_to_task(self.sp, &mut self.task_regs);
        }
    }
}

// Thin shim to force aapcs calling convention
extern "aapcs" fn run_closure<F>(f: F)  where F: FnOnce() {
    f();
    yieldk();
}


#[naked]
#[no_mangle]
pub unsafe extern "C" fn SVCall() {
    asm!("
    cmp lr, #0xfffffff9
    bne to_kernel

    /* TODO: Set thread mode to unprivileged */
    movw lr, #0xfffd
    movt lr, #0xffff
    bx lr

  to_kernel:
    /* TODO: Set thread mode to privileged */
    movw LR, #0xFFF9
    movt LR, #0xFFFF
    bx lr"
    : : : : "volatile");
}


pub unsafe extern "C" fn switch_to_task(mut task_stack: *const usize, process_regs: &mut [usize; 8]) -> *const usize {
    asm!("
    /* Load bottom of stack into Process Stack Pointer */
    msr psp, $0
    /* Load non-hardware-stacked registers from Process stack */
    /* Ensure that $2 is stored in a callee saved register */
    ldmia $2, {r4-r11}
    /* SWITCH */
    svc 0xff /* It doesn't matter which SVC number we use here */
    /* Push non-hardware-stacked registers into Process struct's */
    /* regs field */
    stmia $2, {r4-r11}
    mrs $0, PSP /* PSP into r0 */"
    : "={r0}"(task_stack)
    : "{r0}"(task_stack), "{r1}"(process_regs)
    : "r4","r5","r6","r7","r8","r9","r10","r11" : "volatile" );
    task_stack
}

pub fn yieldk() {
    // Note: A process stops yielding when there is a callback ready to run,
    // which the kernel executes by modifying the stack frame pushed by the
    // hardware. The kernel copies the PC value from the stack frame to the LR
    // field, and sets the PC value to callback to run. When this frame is
    // unstacked during the interrupt return, the effectively clobbers the LR
    // register.
    //
    // At this point, the callback function is now executing, which may itself
    // clobber any of the other caller-saved registers. Thus we mark this
    // inline assembly as conservatively clobbering all caller-saved registers,
    // forcing yield to save any live registers.
    //
    // Upon direct observation of this function, the LR is the only register
    // that is live across the SVC invocation, however, if the yield call is
    // inlined, it is possible that the LR won't be live at all (commonly seen
    // for the `loop { yieldk(); }` idiom) or that other registers are live,
    // thus it is important to let the compiler do the work here.
    //
    // According to the AAPCS: A subroutine must preserve the contents of the
    // registers r4-r8, r10, r11 and SP (and r9 in PCS variants that designate
    // r9 as v6) As our compilation flags mark r9 as the PIC base register, it
    // does not need to be saved. Thus we must clobber r0-3, r12, and LR
    unsafe {
        asm!(
            "svc 0"
            :
            :
            : "memory", "r0", "r1", "r2", "r3", "r12", "lr"
            : "volatile");
    }
}