pub(crate) mod stream {
    use std::io;
    use std::os::unix::net::{self, SocketAddr};

    pub(crate) fn connect_addr(_: &SocketAddr) -> io::Result<net::UnixStream> {
        os_required!()
    }

    pub(crate) fn pair() -> io::Result<(net::UnixStream, net::UnixStream)> {
        os_required!()
    }
}
