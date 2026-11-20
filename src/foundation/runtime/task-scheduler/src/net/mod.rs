#![cfg(not(loom))]

//! TCP listeners, streams and DNS resolution. Use [`AsyncFd`] for other Linux descriptors.
//!
//! [`AsyncFd`]: crate::io::unix::AsyncFd

mod addr;
cfg_not_wasip1! {
    #[cfg(feature = "net")]
    pub(crate) use addr::to_socket_addrs;
}
pub use addr::ToSocketAddrs;

cfg_net! {
    mod lookup_host;
    pub use lookup_host::lookup_host;

    pub mod tcp;
    pub use tcp::listener::TcpListener;
    pub use tcp::stream::TcpStream;
    cfg_not_wasip1! {
        pub use tcp::socket::TcpSocket;
    }
}
