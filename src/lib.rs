#![no_std]
#![feature(naked_functions, never_type, asm, const_fn)]

mod thread;
mod scheduler;
mod thread_list;
mod mutex;

#[allow(unused_imports)]
use cortex_m_rt;

pub use thread::{Thread, yieldk};
pub use thread_list::ThreadList;
pub use scheduler::{run_threads, get_current_thread, wakeup_thread, wakeup_threads};
pub use mutex::{Mutex, MutexGuard};