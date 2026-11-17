//! Single keys pressed while a run goes on, read without Enter and without
//! echo. Only where stdin is the foreground terminal, on Linux. The guard
//! [`Keys`] restores the terminal when it drops, also while a panic unwinds.
//! Ctrl+C, Ctrl+\ and Ctrl+Z arrive as bytes here, so the terminal is
//! restored before their signal takes effect.

/// Reads keys on a thread of its own until dropped.
pub struct Keys {
    #[cfg(target_os = "linux")]
    stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
    #[cfg(target_os = "linux")]
    thread: Option<std::thread::JoinHandle<()>>,
    #[cfg(target_os = "linux")]
    saved: libc::termios,
}

/// Calls `on_key` with each key pressed, or `None` where keys cannot be read.
#[cfg(not(target_os = "linux"))]
pub fn listen(_on_key: impl FnMut(u8) + Send + 'static) -> Option<Keys> {
    None
}

#[cfg(target_os = "linux")]
fn set(mode: &libc::termios) {
    // SAFETY: fd 0 and a valid termios; a failure only leaves the mode as it was.
    unsafe { libc::tcsetattr(0, libc::TCSANOW, mode) };
}

/// Calls `on_key` with each key pressed, or `None` where keys cannot be read.
#[cfg(target_os = "linux")]
pub fn listen(mut on_key: impl FnMut(u8) + Send + 'static) -> Option<Keys> {
    use std::sync::atomic::Ordering;
    // SAFETY: plain calls on fd 0; the termios is read only when tcgetattr filled it.
    let saved = unsafe {
        // a background job that set the mode would be stopped (SIGTTOU)
        if libc::isatty(0) != 1 || libc::tcgetpgrp(0) != libc::getpgrp() {
            return None;
        }
        let mut mode = std::mem::MaybeUninit::<libc::termios>::uninit();
        if libc::tcgetattr(0, mode.as_mut_ptr()) != 0 {
            return None;
        }
        mode.assume_init()
    };
    let mut raw = saved;
    raw.c_lflag &= !(libc::ICANON | libc::ECHO | libc::ISIG);
    (raw.c_cc[libc::VMIN], raw.c_cc[libc::VTIME]) = (1, 0);
    set(&raw);
    let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let stopped = stop.clone();
    let thread = std::thread::spawn(move || {
        while !stopped.load(Ordering::Relaxed) {
            let mut fd = libc::pollfd { fd: 0, events: libc::POLLIN, revents: 0 };
            let mut byte = 0u8;
            // SAFETY: one valid pollfd; a one-byte buffer of one byte.
            unsafe {
                // a tenth of a second, so a stop is seen soon
                if libc::poll(&mut fd, 1, 100) <= 0 {
                    continue;
                }
                if libc::read(0, (&raw mut byte).cast(), 1) != 1 {
                    return;
                }
            }
            let signal = match byte {
                3 => libc::SIGINT,
                0x1c => libc::SIGQUIT,
                0x1a => libc::SIGTSTP,
                key => {
                    on_key(key);
                    continue;
                }
            };
            set(&saved);
            eprintln!();
            // SAFETY: raising a standard signal. Only a stop returns, once continued.
            unsafe { libc::raise(signal) };
            set(&raw);
        }
    });
    Some(Keys { stop, thread: Some(thread), saved })
}

#[cfg(target_os = "linux")]
impl Drop for Keys {
    fn drop(&mut self) {
        self.stop.store(true, std::sync::atomic::Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
        set(&self.saved);
    }
}
