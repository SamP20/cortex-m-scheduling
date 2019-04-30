#![no_std]
#![feature(naked_functions, never_type, asm, const_fn)]

mod thread;
mod scheduler;
mod thread_list;
mod mutex;

#[allow(unused_imports)]
use cortex_m_rt;

pub use thread::{
    Thread,
    ThreadCreateError,
    SwitchReason,
    ThreadID,
    get_current_thread,
    threads_waiting,
    yieldk
};

pub use thread_list::ThreadList;
pub use scheduler::run_threads;
pub use mutex::{Mutex, MutexGuard};