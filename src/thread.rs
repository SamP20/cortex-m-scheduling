use core::{mem, ptr};
use core::sync::atomic::{AtomicU8, Ordering};
use super::ThreadList;

const INVALID_THREAD_ID: u8 = 0xff;

static CURRENT_THREAD: AtomicU8 = AtomicU8::new(INVALID_THREAD_ID);

// Bitfield of threads that have been woken up by interrupts
static THREAD_WAKEUPS: ThreadList = ThreadList::new();

// This is safe because threads can only be created on the main thread
static mut TOTAL_THREADS: u8 = 0;


/// This is used in the syscall handler. When set to 1 this means the
/// svc_handler was called.
#[no_mangle]
#[used]
static mut SYSCALL_FIRED: usize = 0;

/// This is called in the hard fault handler. When set to 1 this means the hard
/// fault handler was called.
///
/// n.b. If the kernel hard faults, it immediately panic's. This flag is only
/// for handling application hard faults.
#[no_mangle]
#[used]
static mut THREAD_HARD_FAULT: usize = 0;

#[derive(Debug)]
pub enum ThreadCreateError {
    TooManyThreads,
    StackTooSmall,
    NotOnMainThread,
}


pub enum SwitchReason {
    Yield,
    Fault,
    Finished,
    NotReady,
    Unknown
}


#[derive(Copy, Clone)]
pub struct ThreadID {
    index: u8
}


impl ThreadID {
    pub unsafe fn new(index: u8) -> Self {
        Self{ index }
    }

    pub fn raw(&self) -> u8 {
        self.index
    }

    pub fn schedule(&self) {
        THREAD_WAKEUPS.add(*self);
    }
}


pub struct Thread<'a> {
    _stack: &'a mut [usize],
    thread_regs: [usize; 8],
    sp: *mut usize, //Mutable pointer makes this type not thread safe, which is what we want
    id: ThreadID,
}


impl<'a> Thread<'a> {
    pub fn new<F>(stack: &'a mut [usize], func: F) -> Result<Self, ThreadCreateError>
        where F: FnOnce() + 'a + Send
    {
        if CURRENT_THREAD.load(Ordering::Acquire) != INVALID_THREAD_ID {
            return Err(ThreadCreateError::NotOnMainThread);
        }

        let index = unsafe { TOTAL_THREADS };
        if index >= 32 {
            return Err(ThreadCreateError::TooManyThreads);
        }

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
        stack[top - 3] = thread_end as usize; // LR
        stack[top - 4] = 0xCCCCCCCC; // R12
        stack[top - 5] = 0x33333333; // R3
        stack[top - 6] = 0x22222222; // R2
        stack[top - 7] = 0x11111111; // R1
        stack[top - 8] = 0x00000000; // R0

        let sp = unsafe { stack.as_mut_ptr().offset(top as isize - 8) };

        unsafe {
            ptr::copy_nonoverlapping(data_ptr, sp, dsize);
        }

        let thread_regs = [
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


        unsafe { TOTAL_THREADS += 1 };
        let id = unsafe { ThreadID::new(index) };
        id.schedule();

        Ok(Self { _stack: stack, thread_regs, sp, id })
    }

    pub fn switch_to(&mut self) -> SwitchReason {
        if THREAD_WAKEUPS.remove(self.id) {
            self.switch_internal()
        } else {
            SwitchReason::NotReady
        }
    }

    pub fn force_switch_to(&mut self) -> SwitchReason {
        THREAD_WAKEUPS.remove(self.id);
        self.switch_internal()
    }

    fn switch_internal(&mut self) -> SwitchReason {
        unsafe {
            // It's impossible to call this from another thread since the
            // Thread type isn't Send, but the thread closure requires Send.
            CURRENT_THREAD.store(self.get_id().raw(), Ordering::Release);
            self.sp = switch_to_thread(self.sp, &mut self.thread_regs);
            CURRENT_THREAD.store(INVALID_THREAD_ID, Ordering::Release);

            let syscall_fired = ptr::read_volatile(&SYSCALL_FIRED);
            ptr::write_volatile(&mut SYSCALL_FIRED, 0);

            let thread_fault = ptr::read_volatile(&THREAD_HARD_FAULT);
            ptr::write_volatile(&mut THREAD_HARD_FAULT, 0);

            if thread_fault  == 1 {
                SwitchReason::Fault
            } else if syscall_fired == 1 {
                let result = get_syscall(self.sp);

                match result.nr {
                    0 => SwitchReason::Yield,
                    1 => SwitchReason::Finished,
                    _ => SwitchReason::Unknown
                }

            } else {
                SwitchReason::Unknown
            }
        }
    }

    pub fn get_id(&self) -> ThreadID {
        self.id
    }

    /// Gets the currently running thread.
    #[inline]
    pub fn get_current() -> ThreadID {
        unsafe { ThreadID::new(CURRENT_THREAD.load(Ordering::Acquire)) }
    }
}


#[inline]
pub fn threads_waiting() -> bool {
    THREAD_WAKEUPS.is_empty()
}


#[inline]
pub(crate) fn schedule_all(ids: u32) {
    THREAD_WAKEUPS.add_all(ids);
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


// Thin shim to force aapcs calling convention
extern "aapcs" fn run_closure<F>(f: F)
    where F: FnOnce() {
    f();
}

// Naked because there is no stack remaining when this gets called and we don't
// want to risk stack underflow caused by auto generated instructions.
#[naked]
extern "aapcs" fn thread_end() {
    unsafe {
        loop {
            asm!(
                "svc 1"
                :
                :
                : "memory", "r0", "r1", "r2", "r3", "r12", "lr"
                : "volatile");
        }
    }
}

#[naked]
#[no_mangle]
unsafe extern "C" fn SVCall() {
    asm!("
    cmp lr, #0xfffffff9
    bne to_kernel

    /* TODO: Set thread mode to unprivileged */
    movw lr, #0xfffd
    movt lr, #0xffff
    bx lr

  to_kernel:
    ldr r0, =SYSCALL_FIRED
    mov r1, #1
    str r1, [r0, #0]

    /* TODO: Set thread mode to privileged */
    movw LR, #0xFFF9
    movt LR, #0xFFFF
    bx lr"
    : : : : "volatile");
}


unsafe extern "C" fn switch_to_thread(mut thread_stack: *mut usize, process_regs: &mut [usize; 8]) -> *mut usize {
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
    : "={r0}"(thread_stack)
    : "{r0}"(thread_stack), "{r1}"(process_regs)
    : "r4","r5","r6","r7","r8","r9","r10","r11" : "volatile" );
    thread_stack
}

struct SyscallResult {
    nr: u8,
    regs: [usize; 4]
}

unsafe fn get_syscall(stack_pointer: *const usize) -> SyscallResult {
    let mut result = SyscallResult{ nr: 0, regs: [0; 4] };
    ptr::copy_nonoverlapping(stack_pointer, result.regs.as_mut_ptr(), 4);
    let pcptr = ptr::read_volatile((stack_pointer as *const *const u16).offset(6));
    let svc_instr = ptr::read_volatile(pcptr.offset(-1));
    result.nr = (svc_instr & 0xff) as u8;

    result
}

#[allow(dead_code)]
unsafe fn set_syscall_return_value(stack_pointer: *mut usize, return_value: usize) {
    ptr::write_volatile(stack_pointer, return_value);
}