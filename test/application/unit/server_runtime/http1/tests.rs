use super::*;
use futures::FutureExt;
use tokio::io::{AsyncReadExt, AsyncWriteExt, DuplexStream};

fn connection() -> (
    WriteIdleTimeout<DuplexStream>,
    DuplexStream,
    CancellationToken,
) {
    let (stream, peer) = tokio::io::duplex(1);
    let stop = CancellationToken::new();
    let permit = Arc::new(Semaphore::new(1)).try_acquire_owned().unwrap();
    (
        WriteIdleTimeout::new(stream, Duration::from_secs(30), permit, stop.clone()),
        peer,
        stop,
    )
}

#[tokio::test(start_paused = true)]
async fn write_idle_timeout_interrupts_a_stalled_peer() {
    let (mut stream, _peer, _) = connection();
    let error = stream.write_all(b"ab").await.unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::TimedOut);
}

#[tokio::test(start_paused = true)]
async fn vectored_progress_resets_the_scalar_write_deadline() {
    let (mut stream, mut peer, _) = connection();
    stream.write_all(b"a").await.unwrap();
    assert!(stream.write(b"b").now_or_never().is_none());
    tokio::time::advance(Duration::from_secs(29)).await;
    assert_eq!(peer.read_u8().await.unwrap(), b'a');
    assert_eq!(
        stream.write_vectored(&[IoSlice::new(b"b")]).await.unwrap(),
        1
    );
    assert!(stream.write(b"c").now_or_never().is_none());
    tokio::time::advance(Duration::from_secs(2)).await;
    assert!(stream.write(b"c").now_or_never().is_none());
    assert_eq!(peer.read_u8().await.unwrap(), b'b');
    stream.flush().await.unwrap();
    stream.shutdown().await.unwrap();
    assert_eq!(peer.read(&mut [0]).await.unwrap(), 0);
}

#[tokio::test]
async fn forced_shutdown_preempts_ready_output() {
    let (mut writer, _peer, stop) = connection();
    stop.cancel();
    for error in [
        writer.write(b"x").await.unwrap_err(),
        writer
            .write_vectored(&[IoSlice::new(b"y")])
            .await
            .unwrap_err(),
        writer.flush().await.unwrap_err(),
        writer.shutdown().await.unwrap_err(),
    ] {
        assert_eq!(error.kind(), io::ErrorKind::ConnectionAborted);
    }
}

#[test]
fn accept_backoff_is_bounded_and_resets() {
    let mut backoff = AcceptBackoff::default();
    assert_eq!(backoff.failure_delay(), Duration::from_millis(50));
    assert_eq!(backoff.failure_delay(), Duration::from_millis(100));
    for _ in 0..20 {
        assert!(backoff.failure_delay() <= Duration::from_secs(1));
    }
    backoff.reset();
    assert_eq!(backoff.failure_delay(), Duration::from_millis(50));
}

#[tokio::test]
async fn backoff_is_interrupted_by_shutdown() {
    let stop = CancellationToken::new();
    stop.cancel();
    assert!(
        !wait_after_accept_error(
            "example-product",
            "test",
            &stop,
            &mut AcceptBackoff::default(),
            &io::Error::from_raw_os_error(24)
        )
        .await
    );
}
