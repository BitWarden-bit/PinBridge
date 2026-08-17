use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};

struct ShutdownInner {
    requested: AtomicBool,
    wake: Condvar,
    wait_lock: Mutex<()>,
}

#[derive(Clone)]
pub(crate) struct Shutdown {
    inner: Arc<ShutdownInner>,
}

impl Shutdown {
    pub(crate) fn new() -> Self {
        Self {
            inner: Arc::new(ShutdownInner {
                requested: AtomicBool::new(false),
                wake: Condvar::new(),
                wait_lock: Mutex::new(()),
            }),
        }
    }

    #[allow(dead_code)]
    pub(crate) fn request(&self) {
        self.inner.requested.store(true, Ordering::Release);
        self.inner.wake.notify_all();
    }

    pub(crate) fn is_requested(&self) -> bool {
        self.inner.requested.load(Ordering::Acquire)
    }

    pub(crate) fn wait(&self) {
        let mut guard = self.inner.wait_lock.lock().expect("shutdown lock poisoned");
        while !self.is_requested() {
            guard = self.inner.wake.wait(guard).expect("shutdown lock poisoned");
        }
    }
}

#[cfg(windows)]
mod console {
    use super::{Shutdown, ShutdownInner};
    use std::io;
    use std::sync::atomic::Ordering;
    use std::sync::{Arc, OnceLock};
    use windows_sys::Win32::System::Console::{
        SetConsoleCtrlHandler, CTRL_BREAK_EVENT, CTRL_C_EVENT,
    };

    // This is deliberately process-lifetime storage. A Windows console
    // callback can race unregistering, so the object it may observe must not
    // be released when ConsoleHandler is dropped.
    static HANDLER_TARGET: OnceLock<Arc<ShutdownInner>> = OnceLock::new();

    unsafe extern "system" fn handler(event: u32) -> i32 {
        if event != CTRL_C_EVENT && event != CTRL_BREAK_EVENT {
            return 0;
        }
        if let Some(shutdown) = HANDLER_TARGET.get() {
            // The handler only flips the atomic flag and notifies waiters.
            shutdown.requested.store(true, Ordering::Release);
            shutdown.wake.notify_all();
        }
        1
    }

    pub(crate) struct ConsoleHandler;

    pub(crate) fn install(shutdown: &Shutdown) -> io::Result<ConsoleHandler> {
        HANDLER_TARGET.set(shutdown.inner.clone()).map_err(|_| {
            io::Error::new(
                io::ErrorKind::AlreadyExists,
                "console handler already installed",
            )
        })?;
        if unsafe { SetConsoleCtrlHandler(Some(handler), 1) } == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(ConsoleHandler)
    }

    impl Drop for ConsoleHandler {
        fn drop(&mut self) {
            // Do not clear HANDLER_TARGET: a callback already in flight may
            // still read it, and OnceLock keeps the Arc alive until process
            // exit. Removing the OS registration is still safe and prevents
            // future callbacks in normal operation.
            unsafe {
                SetConsoleCtrlHandler(Some(handler), 0);
            }
        }
    }
}

#[cfg(not(windows))]
mod console {
    use super::Shutdown;
    use std::io;

    pub(crate) struct ConsoleHandler;

    pub(crate) fn install(_: &Shutdown) -> io::Result<ConsoleHandler> {
        // The headless binary installs native console handlers on Windows.
        // Other platforms still expose the same testable wait/request
        // abstraction; process supervisors should request termination there.
        Ok(ConsoleHandler)
    }
}

pub(crate) use console::ConsoleHandler;

pub(crate) fn install_console_handler(shutdown: &Shutdown) -> std::io::Result<ConsoleHandler> {
    console::install(shutdown)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Barrier};
    use std::thread;

    #[test]
    fn request_unblocks_wait_without_real_console_signal() {
        let shutdown = Shutdown::new();
        let ready = Arc::new(Barrier::new(2));
        let worker_shutdown = shutdown.clone();
        let worker_ready = ready.clone();
        let worker = thread::spawn(move || {
            worker_ready.wait();
            worker_shutdown.wait();
            worker_shutdown.is_requested()
        });
        ready.wait();
        shutdown.request();
        assert!(worker.join().unwrap());
    }

    #[cfg(windows)]
    #[test]
    fn console_handler_installation_is_singleton() {
        let first = Shutdown::new();
        let handler = install_console_handler(&first).unwrap();
        let second = Shutdown::new();
        assert!(install_console_handler(&second).is_err());
        drop(handler);
    }
}
