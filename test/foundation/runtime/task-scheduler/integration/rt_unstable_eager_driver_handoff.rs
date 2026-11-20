// These tests use Unix socket pairs to generate I/O events from within the same process.
//
// Also, Miri doesn't like it when you leak a thread, which will happen in
// the "deadlock" case below, so we skip these tests on Miri.
#![cfg(all(not(miri), unix, feature = "full"))]

use std::sync::mpsc::RecvTimeoutError;
use std::time::Duration;

/// Test that, without `enable_eager_driver_handoff`, we can reliably reproduce
/// a deadlock when a task blocks indefinitely. If this test fails, it means
/// that the test `eager_driver_handoff_fixes_deadlock` is not actually testing
/// a condition that can deadlock the runtime.
#[test]
fn deadlocks_consistently() {
    let rt = rt_builder().build().unwrap();
    assert_eq!(
        do_test(rt),
        Err(RecvTimeoutError::Timeout),
        "runtime did not deadlock! the `eager_driver_handoff_fixes_deadlock` \
         test may no longer reproduce the bug it is intended to test a fix \
         for!",
    );
}

/// This is the one that actually tests whether eager driver handoff works as
/// expected: it runs the same reproducer as `deadlocks_consistently` a single
/// timebut with `enable_eager_driver_handoff` enabled. If this test fails, it
/// means that the eager driver handoff fix is not working as expected.
#[test]
#[cfg(tokio_unstable)]
fn eager_driver_handoff_fixes_deadlock() {
    let rt = rt_builder().enable_eager_driver_handoff().build().unwrap();
    assert_eq!(
        do_test(rt),
        Ok(()),
        "the runtime should not deadlock because the driver is \
         eagerly handed off"
    );
}

/// Reproduces a deadlock occurring when the worker thread holding the I/O or
/// time driver runs a task that blocks that worker until another task, which is
/// *waiting on the I/O or time driver*, performs an action that unblocks it.
/// This general class of problem is described in:
/// <https://github.com/tokio-rs/tokio/issues/4730>.
///
/// The deadlock occurs as follows:
///
/// 1. Both worker threads are idle. Worker A parks on the I/O driver (holding
///    the driver lock), Worker B parks on a condvar.
/// 2. An I/O event fires. Worker A (the driver holder) wakes up and processes the
///    I/O  event, placing the woken task in its run queue.
/// 3. Worker A begins polling the task. The task blocks the worker thread
///    until it is woken by the other task, so Worker A never returns to
///    poll the I/O driver.
/// 4. Worker B remains parked on the condvar indefinitely — nobody calls
///    `unpark` on it, so it never wakes to take over the I/O driver.
/// 5. Subsequent I/O events are never processed because no worker is polling
///    the time driver. The runtime is wedged.
///
/// To trigger this reliably, the test uses two tasks which send and receive
/// through Unix socket pairs.
///
/// The "bad" task's socket is written to immediately, while the "good" task is
/// still waiting for a read on its socket. Then, when it's woken, it writes to
/// the other socket, waking the "good" task, and then blocks on a notification
/// from a blocking channel which is only written to by the "good" task.
///
/// Because this task first waits on a socket read, this ensures that it is
/// first polled from the worker thread that has last parked on the I/O
/// driver, so when it blocks, it prevents the driver from making progress.
///
/// The specific scenario we've constructed here is, admittedly, somewhat
/// contrived: most Tokio applications aren't going to spawn tasks that attempt
/// to communicate using a `std::sync::mpsc` blocking channel. Instead, you can
/// imagine the blocking channel as a stand-in for some other operation that
/// blocks a thread until some event occurs, which is triggered by an operation
/// performed by the other task, which is waiting on an asynchronous event
/// before performing it. The task that blocks the runtime could, for example,
/// be waiting on a blocking syscall that completes only when the other task
/// does something.
fn do_test(rt: tokio::runtime::Runtime) -> Result<(), RecvTimeoutError> {
    use std::io::{Read, Write};
    use std::os::unix::net::UnixStream;
    use tokio::io::{Interest, unix::AsyncFd};

    fn socket_pair() -> std::io::Result<(AsyncFd<UnixStream>, AsyncFd<UnixStream>)> {
        let (sender, receiver) = UnixStream::pair()?;
        sender.set_nonblocking(true)?;
        receiver.set_nonblocking(true)?;
        Ok((AsyncFd::new(sender)?, AsyncFd::new(receiver)?))
    }

    // A blocking MPSC from the standard library is used to wait for the test to
    // complete within a reasonable amount of time, so that we can determine
    // whether or not the runtime has deadlocked.
    let (done_tx, done_rx) = std::sync::mpsc::channel();

    std::thread::spawn(move || {
        rt.block_on(async {
            let (sender1, receiver1) = socket_pair().unwrap();
            let (sender2, receiver2) = socket_pair().unwrap();

            // Note that we use a *blocking* MPSC from the standard library
            // here, since the entire purpose of the channel is to temporarily
            // block a worker thread when calling `recv`.
            let (deadlock_tx, deadlock_rx) = std::sync::mpsc::channel();

            let bad_task = tokio::spawn(async move {
                // Wait on a socket for a bit to ensure that we end up on the worker
                // holding the time driver.
                let mut buf = [0u8; 1];
                receiver1
                    .async_io(Interest::READABLE, |mut socket| socket.read_exact(&mut buf))
                    .await
                    .unwrap();
                // Okay, we have now definitely woken up on the worker
                // thread that polled the I/0 driver. Now, block this
                // worker thread until woken by the *other* task. If the
                // other task's I/O driver notification wakes it up,
                // this task will be unblocked and the runtime will keep running.
                //
                // However, if *this* task blocking the thread prevents the
                // I/O driver from ever running, the runtime will be wedged forever.
                sender2
                    .async_io(Interest::WRITABLE, |mut socket| socket.write_all(&[2]))
                    .await
                    .unwrap();
                deadlock_rx.recv().unwrap();
            });

            let good_task = tokio::spawn(async move {
                let mut buf = [0u8; 1];
                receiver2
                    .async_io(Interest::READABLE, |mut socket| socket.read_exact(&mut buf))
                    .await
                    .unwrap();
                deadlock_tx.send(()).unwrap();
            });

            tokio::time::sleep(Duration::from_millis(100)).await;

            sender1
                .async_io(Interest::WRITABLE, |mut socket| socket.write_all(&[1]))
                .await
                .unwrap();

            good_task.await.unwrap();
            bad_task.await.unwrap();
        });

        done_tx.send(()).unwrap();
    });

    done_rx.recv_timeout(Duration::from_secs(10))
}

/// Base runtime builder for both tests in this module: two worker threads, time
/// driver enabled. This returns a `Builder`, rather than a `Runtime`, so that
/// the `eager_driver_handoff_fixes_deadlock` test can configure the runtime to
/// enable eager driver handoff before building it.
fn rt_builder() -> tokio::runtime::Builder {
    let mut builder = tokio::runtime::Builder::new_multi_thread();
    builder.enable_all().worker_threads(2);
    builder
}
