# Overview and aims

This library is a thread scheduling library designed for co-operative or preemptive multithreading
without any heap allocation. Threads are created from a stack array and closure which can take
ownership of any Send type. These threads can then be scheduled as desired by the main thread,
whether that be round robin, or your own custom implementation.

This library also provides a Mutex synchronisation type which allows two or more threads to take
an immutable reference to a Mutex<T> and then lock it for the duration required. This type also
informs the scheduler when waiting threads can try to lock again.


# The Thread type

The `Thread` type is the core structure of this library. Constructing a `Thread` requires a stack and
a function to call.

```
fn new<F>(stack: &'a mut [usize], func: F) -> Thread<'a>
    where F: FnOnce() + 'a + Send
```

The stack is a suitably sized slice which will be used to store the stack of the thread. `func` is an
`FnOnce` which it can take ownership of items from the surrounding context. This is the core concept
that makes this library unique. Behind the scenes this is implemented by initializing the stack with
the `func` as if it were an interrupted "C" (more specifically [aapcs]) function call. When the thread
is resumed, the r0-r3 registers will be popped from the stack, and this will magically behave as if you
called the function directly!

[aaps]: http://infocenter.arm.com/help/topic/com.arm.doc.ihi0042f/IHI0042F_aapcs.pdf

There are also methods for getting the current `ThreadID` which can be used to schedule the thread.

# The Scheduler

The current scheduler is extremely simple. It loops through all threads calling their `switch_to` method.
`switch_to` will only perform the switch if the thread is pending (use `force_switch_to` if you want to
force it). Afterwards it will check if any threads are pending, and if not it will put the processor to
sleep.

# The Mutex

TODO: Explain how the mutex works