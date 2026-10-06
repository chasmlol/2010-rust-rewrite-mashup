//! Work that must finish before the process leaves (a trace flush, a bench
//! report, screenshot writes), and the exit that runs it.
//!
//! `std::process::exit` runs the C library's exit handlers, the GPU driver's
//! teardown among them, while render and pipeline-compile threads may still
//! be inside the driver: the process then dies on a segfault, and the sound
//! server keeps looping the audio it last had while the core is written.
//! [`exit_now`] runs the hooks registered here and leaves without that
//! teardown. A hook still runs, once, on any other exit that does run the
//! C library's handlers.

use std::sync::Mutex;

static HOOKS: Mutex<Vec<fn()>> = Mutex::new(Vec::new());

/// Runs `hook` before the process exits; hooks run last-registered first.
pub fn at_exit(hook: fn()) {
    HOOKS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .push(hook);
    arm_libc_exit();
}

pub fn run_exit_hooks() {
    let hooks = std::mem::take(
        &mut *HOOKS
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner),
    );
    for hook in hooks.into_iter().rev() {
        let _ = std::panic::catch_unwind(hook);
    }
}

pub fn exit_now(code: i32) -> ! {
    run_exit_hooks();
    #[cfg(unix)]
    {
        unsafe extern "C" {
            fn _exit(code: i32) -> !;
        }
        unsafe { _exit(code) }
    }
    #[cfg(not(unix))]
    std::process::exit(code)
}

#[cfg(unix)]
fn arm_libc_exit() {
    use std::sync::atomic::{AtomicBool, Ordering};
    static ARMED: AtomicBool = AtomicBool::new(false);
    if ARMED.swap(true, Ordering::SeqCst) {
        return;
    }
    unsafe extern "C" {
        fn atexit(callback: extern "C" fn()) -> i32;
    }
    extern "C" fn run_at_exit() {
        run_exit_hooks();
    }
    let _ = unsafe { atexit(run_at_exit) };
}

#[cfg(not(unix))]
fn arm_libc_exit() {}
