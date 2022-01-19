/* Copyright 2022 Ivan Boldyrev
 *
 * Licensed under the MIT License.
 * Permission is hereby granted, free of charge, to any person obtaining a copy
 * of this software and associated documentation files (the "Software"), to deal
 * in the Software without restriction, including without limitation the rights
 * to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
 * copies of the Software, and to permit persons to whom the Software is
 * furnished to do so, subject to the following conditions:
 *
 * The above copyright notice and this permission notice shall be included in
 * all copies or substantial portions of the Software.
 *
 * THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
 * IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
 * FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
 * AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
 * LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING
 * FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS
 * IN THE SOFTWARE.
 */

#![doc = include_str!("../README.md")]
use std::{
    io,
    sync::atomic::{AtomicBool, Ordering},
};

/** See crate-level documentation for more info. */
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

#[cfg(test)]
mod tests {
    use std::{
        io::{self, ErrorKind, Read, Write},
        sync::{
            atomic::{AtomicBool, Ordering},
            Arc,
        },
    };

    use crate::Interruptable;

    struct Mock {
        value: Option<io::Result<Vec<u8>>>,
        interrupt: Option<Arc<AtomicBool>>,
    }

    impl Mock {
        fn new(value: io::Result<Vec<u8>>, interrupt: Option<Arc<AtomicBool>>) -> Self {
            Self {
                value: Some(value),
                interrupt,
            }
        }
    }

    impl Read for Mock {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            if let Some(int) = &self.interrupt {
                int.store(true, Ordering::SeqCst);
            }
            match self.value.take() {
                None => Err(io::Error::from(io::ErrorKind::UnexpectedEof)),
                Some(Ok(value)) => {
                    let len = std::cmp::min(buf.len(), value.len());
                    buf.copy_from_slice(&value[..len]);
                    Ok(len)
                }
                Some(Err(e)) => Err(e),
            }
        }
    }

    impl Write for Mock {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            match self.value.take() {
                Some(Ok(_)) => Ok(buf.len()),
                Some(Err(e)) => Err(e),
                None => Err(io::Error::from(ErrorKind::BrokenPipe)),
            }
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn test_read_normal() {
        let flag = Arc::new(AtomicBool::new(false));
        let flag2 = flag.clone();
        let mut inp = Interruptable::new(Mock::new(Ok(vec![42; 100]), None), flag2);
        let mut buf = vec![0; 42];

        assert!(matches!(inp.read(&mut buf), Ok(42)));
        assert_eq!(buf, vec![42; 42]);
    }

    #[test]
    fn test_read_error() {
        let flag = Arc::new(AtomicBool::new(false));
        let mut inp = Interruptable::new(
            Mock::new(Err(io::Error::from(io::ErrorKind::BrokenPipe)), None),
            flag,
        );
        let mut buf = vec![0; 42];

        let e = inp.read(&mut buf).unwrap_err();
        assert_eq!(e.kind(), io::ErrorKind::BrokenPipe);
    }

    #[test]
    fn test_read_pre_interrupt() {
        let flag = Arc::new(AtomicBool::new(false));
        let flag2 = flag.clone();
        let mut inp = Interruptable::new(Mock::new(Ok(vec![42; 100]), None), flag2);
        let mut buf = vec![0; 42];
        flag.store(true, Ordering::SeqCst);

        let e = inp.read(&mut buf).unwrap_err();
        assert_eq!(e.kind(), io::ErrorKind::Other);
        assert!(e.get_ref().is_some());
    }

    #[test]
    fn test_read_incall_interrupt() {
        let flag = Arc::new(AtomicBool::new(false));
        let flag2 = flag.clone();
        let flag3 = flag.clone();
        let mut inp = Interruptable::new(
            Mock::new(
                Err(io::Error::from(io::ErrorKind::Interrupted)),
                Some(flag3),
            ),
            flag2,
        );
        let mut buf = vec![0; 42];
        flag.store(true, Ordering::SeqCst);

        let e = inp.read(&mut buf).unwrap_err();
        assert_eq!(e.kind(), io::ErrorKind::Other);
        assert!(e.get_ref().is_some());
    }

    #[test]
    fn test_read_unhandled_interrupt() {
        let flag = Arc::new(AtomicBool::new(false));
        let flag2 = flag.clone();
        let mut inp = Interruptable::new(
            Mock::new(Err(io::Error::from(io::ErrorKind::Interrupted)), None),
            flag2,
        );
        let mut buf = vec![0; 42];

        let e = inp.read(&mut buf).unwrap_err();
        assert_eq!(e.kind(), io::ErrorKind::Interrupted);
    }

    #[test]
    fn test_write_normal() {
        let flag = Arc::new(AtomicBool::new(false));
        let mut inp = Interruptable::new(Mock::new(Ok(vec![42; 0]), None), flag);
        let buf = vec![0; 42];

        assert!(matches!(inp.write(&buf), Ok(42)));
    }

    #[test]
    fn test_write_error() {
        let flag = Arc::new(AtomicBool::new(false));
        let mut inp = Interruptable::new(
            Mock::new(Err(io::Error::from(io::ErrorKind::BrokenPipe)), None),
            flag,
        );
        let buf = vec![0; 42];

        let e = inp.write(&buf).unwrap_err();
        assert_eq!(e.kind(), io::ErrorKind::BrokenPipe);
    }

    #[test]
    fn test_write_pre_interrupt() {
        let flag = Arc::new(AtomicBool::new(false));
        let flag2 = flag.clone();
        let mut inp = Interruptable::new(Mock::new(Ok(vec![42; 100]), None), flag2);
        let buf = vec![0; 42];
        flag.store(true, Ordering::SeqCst);

        let e = inp.write(&buf).unwrap_err();
        assert_eq!(e.kind(), io::ErrorKind::Other);
        assert!(e.get_ref().is_some());
    }

    #[test]
    fn test_write_incall_interrupt() {
        let flag = Arc::new(AtomicBool::new(false));
        let flag2 = flag.clone();
        let flag3 = flag.clone();
        let mut inp = Interruptable::new(
            Mock::new(
                Err(io::Error::from(io::ErrorKind::Interrupted)),
                Some(flag3),
            ),
            flag2,
        );
        let buf = vec![0; 42];
        flag.store(true, Ordering::SeqCst);

        let e = inp.write(&buf).unwrap_err();
        assert_eq!(e.kind(), io::ErrorKind::Other);
        assert!(e.get_ref().is_some());
    }

    #[test]
    fn test_write_unhandled_interrupt() {
        let flag = Arc::new(AtomicBool::new(false));
        let flag2 = flag.clone();
        let mut inp = Interruptable::new(
            Mock::new(Err(io::Error::from(io::ErrorKind::Interrupted)), None),
            flag2,
        );
        let buf = vec![0; 42];

        let e = inp.write(&buf).unwrap_err();
        assert_eq!(e.kind(), io::ErrorKind::Interrupted);
    }
}
