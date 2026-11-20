//! Non-blocking TCP sockets and the Unix stream used by the Linux signal driver.
//!
//! Follow [`Poll`](crate::Poll)'s readiness rules: an event is a hint to attempt I/O, and the
//! operation can still return `WouldBlock`.

mod tcp;
pub use self::tcp::{TcpListener, TcpStream};

#[cfg(unix)]
mod uds;
#[cfg(unix)]
pub use self::uds::UnixStream;
