use std::{
    io,
    sync::atomic::{AtomicBool, Ordering},
};

pub struct Interruptable<IO, H> {
    inner: IO,
    interrupt_flag: H,
}

impl<IO, H: AsRef<AtomicBool>> Interruptable<IO, H> {
    #[inline]
    pub fn new(inner: IO, interrupt_flag: H) -> Self {
        Self {
            inner,
            interrupt_flag,
        }
    }

    #[inline]
    fn check_again(&self, e: io::Error) -> io::Error {
        if e.kind() == io::ErrorKind::Interrupted
            // It can be interrupted by other signal, so let's check the flag...
            && self.interrupt_flag.as_ref().load(Ordering::SeqCst)
        {
            Self::das_error()
        } else {
            e
        }
    }

    #[inline]
    fn das_error() -> io::Error {
        io::Error::new(
            io::ErrorKind::Other,
            io::Error::from(io::ErrorKind::Interrupted),
        )
    }
}

impl<IO: io::Read, H: AsRef<AtomicBool>> io::Read for Interruptable<IO, H> {
    #[inline]
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.interrupt_flag.as_ref().load(Ordering::SeqCst) {
            Err(Self::das_error())
        } else {
            self.inner.read(buf).map_err(|e| self.check_again(e))
        }
    }
}

impl<IO: io::Write, H: AsRef<AtomicBool>> io::Write for Interruptable<IO, H> {
    #[inline]
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if self.interrupt_flag.as_ref().load(Ordering::SeqCst) {
            Err(Self::das_error())
        } else {
            self.inner.write(buf).map_err(|e| self.check_again(e))
        }
    }

    #[inline]
    fn flush(&mut self) -> io::Result<()> {
        if self.interrupt_flag.as_ref().load(Ordering::SeqCst) {
            Err(Self::das_error())
        } else {
            self.inner.flush().map_err(|e| self.check_again(e))
        }
    }
}
