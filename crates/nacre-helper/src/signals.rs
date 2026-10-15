//! Event-driven signal wait shared by selector and stream helpers.
use crate::Result;
use nix::sys::{
    signal::{pthread_sigmask, SigSet, SigmaskHow, Signal},
    signalfd::{SfdFlags, SignalFd},
};
pub struct Signals {
    previous: SigSet,
    pub fd: SignalFd,
}
impl Signals {
    pub fn new() -> Result<Self> {
        let mut set = SigSet::empty();
        set.add(Signal::SIGINT);
        set.add(Signal::SIGTERM);
        let mut previous = SigSet::empty();
        pthread_sigmask(SigmaskHow::SIG_BLOCK, Some(&set), Some(&mut previous))?;
        match SignalFd::with_flags(&set, SfdFlags::SFD_CLOEXEC | SfdFlags::SFD_NONBLOCK) {
            Ok(fd) => Ok(Self { previous, fd }),
            Err(error) => {
                pthread_sigmask(SigmaskHow::SIG_SETMASK, Some(&previous), None)?;
                Err(error.into())
            }
        }
    }
}
impl Drop for Signals {
    fn drop(&mut self) {
        if let Err(error) = pthread_sigmask(SigmaskHow::SIG_SETMASK, Some(&self.previous), None) {
            eprintln!("Selector signal mask: {error}");
        }
    }
}
