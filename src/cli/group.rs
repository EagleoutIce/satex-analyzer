//! A build's processes as one group, so that none outlives the build: `satex compile` runs
//! latexmk, which runs the engine, BibTeX, makeindex and what the document's shell escape starts.
//! Ending only the process one started leaves the rest running without a parent.
//!
//! Who starts a build ([`lead`]) makes it the leader of a new process group and ends that
//! group whole ([`end`]). The kernel tells the leader when its starter ends, however it ends
//! (`PR_SET_PDEATHSIG`, Linux), and a `satex compile` that leads a group ends the group with
//! itself ([`die_together`]). Unix only: elsewhere a build ends as a single process.
use std::process::{Child, Command};

/// `command` as the leader of a new process group, told (SIGTERM) when this process ends.
pub fn lead(command: &mut Command) {
    #[cfg(unix)]
    std::os::unix::process::CommandExt::process_group(command, 0);
    #[cfg(target_os = "linux")]
    // SAFETY: runs between fork and exec, where only async-signal-safe calls are allowed:
    // `prctl` is one and touches no memory of the parent.
    unsafe {
        std::os::unix::process::CommandExt::pre_exec(command, || {
            libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM);
            Ok(())
        });
    }
}

/// Ends the group `child` leads ([`lead`]) and waits for the child.
pub fn end(child: &mut Child) {
    #[cfg(unix)]
    // SAFETY: `killpg` only sends a signal. The child is not waited for yet, so its number
    // still names it and the group it leads, never a later process.
    unsafe {
        libc::killpg(child.id() as libc::pid_t, libc::SIGKILL);
    }
    let _ = child.kill();
    let _ = child.wait();
}

/// For a build (`satex compile`): when this process leads its group (started by [`lead`], or a
/// shell's job), SIGTERM and SIGHUP end the whole group, so every tool the build started ends
/// with it. In another's group the signals stay as they are: the group is not this build's.
pub fn die_together() {
    #[cfg(unix)]
    {
        extern "C" fn all(_: libc::c_int) {
            // SAFETY: `kill` is async-signal-safe; process 0 names the caller's own group.
            unsafe {
                libc::kill(0, libc::SIGKILL);
            }
        }
        // SAFETY: plain libc calls. The handler only calls `kill`.
        unsafe {
            if libc::getpgrp() == libc::getpid() {
                let handler = all as extern "C" fn(libc::c_int) as libc::sighandler_t;
                libc::signal(libc::SIGTERM, handler);
                libc::signal(libc::SIGHUP, handler);
            }
        }
    }
}
