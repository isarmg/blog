//! Bounded HTTP/1 transport with connection limits, deadlines and graceful shutdown.
//! No filesystem, authentication, operation, or product-state semantics live here.

use crate::log::{CommonEvent, LogRecord};
use rustix::event::{PollFd, PollFlags, Timespec, poll};
use std::{
    future::Future,
    io::{self, IoSlice},
    net::{SocketAddr, TcpListener as StdTcpListener},
    pin::Pin,
    sync::Arc,
    task::{Context as TaskContext, Poll},
    time::Duration,
};
use tokio::{
    io::{AsyncRead, AsyncWrite, ReadBuf, unix::AsyncFd},
    net::TcpStream,
    sync::{OwnedSemaphorePermit, Semaphore},
    time::sleep,
};
use tokio_util::sync::{CancellationToken, WaitForCancellationFutureOwned};

const ACCEPT_BACKOFF_INITIAL: Duration = Duration::from_millis(50);
const ACCEPT_BACKOFF_MAX: Duration = Duration::from_secs(1);

pub(crate) struct WriteIdleTimeout<T> {
    inner: T,
    timeout: Duration,
    timer: Option<Pin<Box<tokio::time::Sleep>>>,
    // The permit must follow the socket even when HTTP upgrades transfer it
    // out of the connection future (e.g. a managed WebSocket).
    _permit: OwnedSemaphorePermit,
    forced: Pin<Box<WaitForCancellationFutureOwned>>,
}

impl<T> WriteIdleTimeout<T> {
    pub(crate) fn new(
        inner: T,
        timeout: Duration,
        permit: OwnedSemaphorePermit,
        force: CancellationToken,
    ) -> Self {
        debug_assert!(!timeout.is_zero());
        Self {
            inner,
            timeout,
            timer: None,
            _permit: permit,
            forced: Box::pin(force.cancelled_owned()),
        }
    }

    fn check_force(&mut self, context: &mut TaskContext<'_>) -> io::Result<()> {
        if self.forced.as_mut().poll(context).is_ready() {
            return Err(io::Error::new(
                io::ErrorKind::ConnectionAborted,
                "HTTP transport forced shutdown",
            ));
        }
        Ok(())
    }

    fn poll_output<R>(
        &mut self,
        context: &mut TaskContext<'_>,
        operation: impl FnOnce(Pin<&mut T>, &mut TaskContext<'_>) -> Poll<io::Result<R>>,
    ) -> Poll<io::Result<R>>
    where
        T: Unpin,
    {
        self.check_force(context)?;
        if let Poll::Ready(result) = operation(Pin::new(&mut self.inner), context) {
            self.timer = None;
            return Poll::Ready(result);
        }
        let timer = self
            .timer
            .get_or_insert_with(|| Box::pin(sleep(self.timeout)));
        if timer.as_mut().poll(context).is_ready() {
            Poll::Ready(Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "HTTP response write made no progress before the idle deadline",
            )))
        } else {
            Poll::Pending
        }
    }
}

impl<T: AsyncRead + Unpin> AsyncRead for WriteIdleTimeout<T> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        context: &mut TaskContext<'_>,
        buffer: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        self.check_force(context)?;
        Pin::new(&mut self.inner).poll_read(context, buffer)
    }
}

impl<T: AsyncWrite + Unpin> AsyncWrite for WriteIdleTimeout<T> {
    fn poll_write(
        mut self: Pin<&mut Self>,
        context: &mut TaskContext<'_>,
        buffer: &[u8],
    ) -> Poll<io::Result<usize>> {
        self.poll_output(context, |stream, cx| stream.poll_write(cx, buffer))
    }

    fn poll_write_vectored(
        mut self: Pin<&mut Self>,
        context: &mut TaskContext<'_>,
        buffers: &[IoSlice<'_>],
    ) -> Poll<io::Result<usize>> {
        self.poll_output(context, |stream, cx| {
            stream.poll_write_vectored(cx, buffers)
        })
    }

    fn is_write_vectored(&self) -> bool {
        self.inner.is_write_vectored()
    }

    fn poll_flush(mut self: Pin<&mut Self>, context: &mut TaskContext<'_>) -> Poll<io::Result<()>> {
        self.poll_output(context, AsyncWrite::poll_flush)
    }

    fn poll_shutdown(
        mut self: Pin<&mut Self>,
        context: &mut TaskContext<'_>,
    ) -> Poll<io::Result<()>> {
        self.poll_output(context, AsyncWrite::poll_shutdown)
    }
}

pub(crate) async fn accept_with_backoff(
    listener: &AsyncFd<StdTcpListener>,
    service: &str,
    listener_addr: &str,
    shutdown: &CancellationToken,
    connection_slots: &Arc<Semaphore>,
    backoff: &mut AcceptBackoff,
) -> Option<(TcpStream, OwnedSemaphorePermit)> {
    loop {
        let readiness = tokio::select! {
            biased;
            () = shutdown.cancelled() => return None,
            result = listener.readable() => result,
        };
        let mut readiness = match readiness {
            Ok(readiness) => readiness,
            Err(err) => {
                if !wait_after_accept_error(service, listener_addr, shutdown, backoff, &err).await {
                    return None;
                }
                continue;
            }
        };

        // AsyncFd deliberately keeps a successful readiness observation cached
        // so callers can drain an fd. We cannot drain a listener before owning
        // capacity, however: waiting for a permit on that stale cache would let
        // an idle listener starve another bind. Recheck the kernel's level state
        // without accepting; clear only the stale observation.
        match listener_is_readable(listener.get_ref()) {
            Ok(true) => {}
            Ok(false) => {
                readiness.clear_ready();
                continue;
            }
            Err(err) => {
                drop(readiness);
                if !wait_after_accept_error(service, listener_addr, shutdown, backoff, &err).await {
                    return None;
                }
                continue;
            }
        }

        // Wait for a global slot only after this listener has a connection
        // ready. An idle bind therefore cannot reserve capacity from another
        // address, while every socket accepted into userspace already owns the
        // permit that bounds its lifetime.
        let connection_permit = tokio::select! {
            biased;
            () = shutdown.cancelled() => return None,
            permit = connection_slots.clone().acquire_owned() => {
                permit.expect("the connection semaphore is never closed")
            }
        };
        if shutdown.is_cancelled() {
            return None;
        }

        let Ok(accepted) = readiness.try_io(|listener| listener.get_ref().accept()) else {
            continue;
        };
        drop(readiness);
        let stream = accepted.and_then(|(stream, _)| {
            stream.set_nonblocking(true)?;
            TcpStream::from_std(stream)
        });
        match stream {
            Ok(stream) => {
                backoff.reset();
                if shutdown.is_cancelled() {
                    return None;
                }
                return Some((stream, connection_permit));
            }
            Err(err) => {
                drop(connection_permit);
                if !wait_after_accept_error(service, listener_addr, shutdown, backoff, &err).await {
                    return None;
                }
            }
        }
    }
}

fn listener_is_readable(listener: &StdTcpListener) -> io::Result<bool> {
    let mut descriptors = [PollFd::new(listener, PollFlags::IN)];
    let no_wait = Timespec::default();
    let ready = poll(&mut descriptors, Some(&no_wait)).map_err(io::Error::from)?;
    Ok(ready > 0 && !descriptors[0].revents().is_empty())
}

async fn wait_after_accept_error(
    service: &str,
    listener_addr: &str,
    shutdown: &CancellationToken,
    backoff: &mut AcceptBackoff,
    err: &io::Error,
) -> bool {
    let retry_delay = backoff.failure_delay();
    log_accept_error(service, listener_addr, err, retry_delay);
    tokio::select! {
        biased;
        () = shutdown.cancelled() => false,
        () = sleep(retry_delay) => true,
    }
}

#[derive(Debug)]
pub(crate) struct AcceptBackoff {
    next_delay: Duration,
}

impl Default for AcceptBackoff {
    fn default() -> Self {
        Self {
            next_delay: ACCEPT_BACKOFF_INITIAL,
        }
    }
}

impl AcceptBackoff {
    fn failure_delay(&mut self) -> Duration {
        let delay = self.next_delay;
        self.next_delay = self.next_delay.saturating_mul(2).min(ACCEPT_BACKOFF_MAX);
        delay
    }

    fn reset(&mut self) {
        self.next_delay = ACCEPT_BACKOFF_INITIAL;
    }
}

fn log_accept_error(
    service: &str,
    listener_addr: &str,
    err: &std::io::Error,
    retry_delay: Duration,
) {
    let category = classify_accept_error(err);
    let record = LogRecord::common(service, CommonEvent::HttpAcceptFailed)
        .and_then(|record| record.with_attribute("listener", listener_addr))
        .and_then(|record| record.with_attribute("category", category))
        .and_then(|record| record.with_attribute("io_kind", format!("{:?}", err.kind())))
        .and_then(|record| record.with_attribute("os_error", serde_json::json!(err.raw_os_error())))
        .and_then(|record| {
            record.with_attribute(
                "retry_ms",
                u64::try_from(retry_delay.as_millis()).expect("bounded accept retry delay"),
            )
        })
        .expect("validated service and bounded transport attributes form a log record");
    emit_record(&record);
}

#[expect(
    clippy::wildcard_enum_match_arm,
    reason = "ErrorKind is an external non-exhaustive error classification"
)]
fn classify_accept_error(err: &std::io::Error) -> &'static str {
    if is_resource_exhaustion(err) {
        "resource"
    } else {
        match err.kind() {
            std::io::ErrorKind::Interrupted => "interrupted",
            std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut => "transient",
            std::io::ErrorKind::ConnectionAborted | std::io::ErrorKind::ConnectionReset => {
                "connection"
            }
            std::io::ErrorKind::PermissionDenied => "permission",
            std::io::ErrorKind::AddrNotAvailable | std::io::ErrorKind::NotConnected => "listener",
            _ => "io",
        }
    }
}

fn is_resource_exhaustion(err: &std::io::Error) -> bool {
    if err.kind() == std::io::ErrorKind::OutOfMemory {
        return true;
    }
    // Linux ENOMEM, ENFILE, EMFILE and ENOBUFS.
    matches!(err.raw_os_error(), Some(12 | 23 | 24 | 105))
}

pub(crate) fn log_connection_error(
    service: &str,
    request_seen: bool,
    err: &(dyn std::error::Error + Send + Sync + 'static),
) {
    let hyper_error = err.downcast_ref::<hyper::Error>();
    let io_error = find_io_error(err);
    let io_kind = io_error.map(std::io::Error::kind);
    let io_disconnect = matches!(
        io_kind,
        Some(
            std::io::ErrorKind::ConnectionReset
                | std::io::ErrorKind::ConnectionAborted
                | std::io::ErrorKind::BrokenPipe
                | std::io::ErrorKind::UnexpectedEof
                | std::io::ErrorKind::NotConnected
        )
    );
    let benign_probe_close =
        hyper_error.is_some_and(hyper::Error::is_incomplete_message) || io_disconnect;
    if !request_seen && benign_probe_close {
        return;
    }

    let category = if hyper_error.is_some_and(hyper::Error::is_parse) {
        "protocol"
    } else if hyper_error.is_some_and(hyper::Error::is_timeout)
        || io_kind == Some(std::io::ErrorKind::TimedOut)
    {
        "timeout"
    } else if hyper_error.is_some_and(|err| err.is_user() || err.is_body_write_aborted()) {
        "service"
    } else if hyper_error
        .is_some_and(|err| err.is_incomplete_message() || err.is_canceled() || err.is_closed())
        || io_disconnect
    {
        "disconnect"
    } else if io_error.is_some() {
        "io"
    } else if hyper_error.is_some() {
        "hyper"
    } else {
        "unknown"
    };
    let io_kind = io_kind.map_or_else(|| "-".to_string(), |kind| format!("{kind:?}"));
    let event = if category == "disconnect" {
        CommonEvent::HttpConnectionClosed
    } else {
        CommonEvent::HttpConnectionFailed
    };
    let record = LogRecord::common(service, event)
        .and_then(|record| record.with_attribute("category", category))
        .and_then(|record| record.with_attribute("request_seen", request_seen))
        .and_then(|record| record.with_attribute("io_kind", io_kind))
        .expect("validated service and bounded transport attributes form a log record");
    emit_record(&record);
}

fn emit_record(record: &LogRecord) {
    if record.emit().is_err() {
        // The same structured record remains observable when the persistent
        // sink fails. No raw error chain, request contents or peer address.
        // There is no further diagnostic sink when stderr is closed too.
        drop(record.emit_stderr());
    }
}

fn find_io_error<'a>(
    err: &'a (dyn std::error::Error + Send + Sync + 'static),
) -> Option<&'a std::io::Error> {
    let mut current: Option<&'a (dyn std::error::Error + 'static)> = Some(err);
    for _ in 0..16 {
        let Some(error) = current else { break };
        if let Some(io_error) = error.downcast_ref::<std::io::Error>() {
            return Some(io_error);
        }
        current = error.source();
    }
    None
}

pub(crate) fn create_listener(addr: SocketAddr) -> io::Result<AsyncFd<StdTcpListener>> {
    use socket2::{Domain, Protocol, Socket, Type};
    let socket = Socket::new(Domain::for_address(addr), Type::STREAM, Some(Protocol::TCP))?;
    if addr.is_ipv6() {
        socket.set_only_v6(true)?;
    }
    socket.set_reuse_address(true)?;
    socket.bind(&addr.into())?;
    socket.listen(1024 /* Default backlog */)?;
    let std_listener = StdTcpListener::from(socket);
    std_listener.set_nonblocking(true)?;
    // SAFETY: AsyncFd exclusively owns this standard listener until deregistration.
    // No aliases or mutable access can close or replace its descriptor.
    #[allow(unsafe_code)]
    let listener = unsafe { AsyncFd::register(std_listener)? };
    Ok(listener)
}

#[path = "../../../../test/application/unit/server_runtime/http1/tests.rs"]
#[cfg(test)]
mod tests;
