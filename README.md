# interruptable
Interruptable `Read` and `Write` wrappers in Rust

## Usage 

This package is to be used for interrupting IO in conjunction with
signal handling crates like `ctrlc`.  These crates do handle received
signals and allow to set some flag, but if application code is performing
IO, it will not able to check the flag until it completes.  The Unix-like
OSes do interrupt IO operations on signals, returning `EINT` errno (aka 
`ErrorKind::Interrupted`), but `std::io` utility methods sliently restart
such operations.

The `Interruptable` wrapper for `std::io::Read` and `std::io::Write` checks
the flag (`std::sync::atomic::AtomicBool`) on each `read` or `write` operation,
and when it sees that flag is set, it returns `std::io::Error` (standard error
for the IO operations) with `std::io::ErrorKind::Other` and nested error of
`ErrorKind::Interrupted`, thus hiding it from the retrying logic.

Life without `interruptabe`:

``` rust,ignore
use std::io;
use std::sync::atomic;
use ctrlc;

let interrupt_flag = atomic::Arc::new(atomic::AtomicBool::new(false));
let interrput_flag2 = interrupt_flag.clone();

ctrlc::set_handler(move || {
    interrput_flag2.store(true, std::sync::atomic::Ordering::SeqCst);
}).unwrap();

let file = io::BufReader::new(
   std::fs::File::open("/path/to/slow/media/data.data")?
);

let my_precious_resource = MyResource::new();

for line in file.lines() {
    // It will be checked only after whole line is read, that may
    // take arbitrary long time.
    if interrput_flag2.load(std::sync::atomic::Ordering::SeqCst) {
        std::mem::drop(my_precious_resource);
        std::process::exit(42);
    }
    let line = line?;
    ...
}
```

Life with `interruptabe`:

``` rust,ignore
use std::io;
use std::sync::atomic;
use ctrlc;

let interrupt_flag = atomic::Arc::new(atomic::AtomicBool::new(false));
let interrput_flag2 = interrupt_flag.clone();

ctrlc::set_handler(move || {
    interrput_flag2.store(true, std::sync::atomic::Ordering::SeqCst);
}).unwrap();

let file = io::BufReader::new(
    // Work both for Read and Write.
    interruptable::Interruptable::new(
        std::fs::File::open("/path/to/slow/media/data.data")?,
        interrupt_flag,
    )
);

let my_precious_resource = MyResource::new();

for line in file.lines() {
    // When the signal will arrive and flag is set, line will be Err(...)
    // immediately, thus you will be able to handle it and gracefully
    // shutdown your application: my_precious_resource is destroyed
    // in the standard way, by Drop::drop.
    let line = line?;
    ...
}
```
