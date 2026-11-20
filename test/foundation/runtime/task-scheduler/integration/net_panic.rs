#![warn(rust_2018_idioms)]
#![cfg(all(feature = "full", not(target_os = "wasi")))]
#![cfg(panic = "unwind")]

use std::error::Error;
use tokio::net::{TcpListener, TcpStream};
use tokio::runtime::{Builder, Runtime};

mod support {
    pub mod panic;
}
use support::panic::test_panic;

#[test]
fn tcp_listener_from_std_panic_caller() -> Result<(), Box<dyn Error>> {
    let std_listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    std_listener.set_nonblocking(true).unwrap();

    let panic_location_file = test_panic(|| {
        let rt = runtime_without_io();
        rt.block_on(async {
            let _ = TcpListener::from_std(std_listener);
        });
    });

    // The panic location should be in this file
    assert_eq!(&panic_location_file.unwrap(), file!());

    Ok(())
}

#[test]
fn tcp_stream_from_std_panic_caller() -> Result<(), Box<dyn Error>> {
    let std_listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();

    let std_stream = std::net::TcpStream::connect(std_listener.local_addr().unwrap()).unwrap();
    std_stream.set_nonblocking(true).unwrap();

    let panic_location_file = test_panic(|| {
        let rt = runtime_without_io();
        rt.block_on(async {
            let _ = TcpStream::from_std(std_stream);
        });
    });

    // The panic location should be in this file
    assert_eq!(&panic_location_file.unwrap(), file!());

    Ok(())
}

// Runtime without `enable_io` so it has no IO driver set.
fn runtime_without_io() -> Runtime {
    Builder::new_current_thread().build().unwrap()
}
