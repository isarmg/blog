use crate::traits::future::{FusedFuture, Future};
use core::pin::Pin;
use core::task::{Context, Poll};
use pin_project_lite::pin_project;

pin_project! {
    /// Future for the [`fuse`](super::FutureExt::fuse) method.
    #[derive(Debug)]
    #[must_use = "futures do nothing unless you `.await` or poll them"]
    pub struct Fuse<Fut> {
        #[pin]
        inner: Option<Fut>,
    }
}

impl<Fut> Fuse<Fut> {
    pub(super) fn new(f: Fut) -> Self {
        Self { inner: Some(f) }
    }
}

impl<Fut: Future> Fuse<Fut> {
    /// Creates a new `Fuse`-wrapped future which is already terminated.
    ///
    /// Use it as the initial state for a pinned future that will be set later.
    ///
    /// # Examples
    ///
    /// ```
    /// use core::pin::pin;
    /// use futures::future::{ready, Fuse, FusedFuture, FutureExt, Ready};
    ///
    /// let mut future = pin!(Fuse::<Ready<u32>>::terminated());
    /// assert!(future.is_terminated());
    /// future.set(ready(5).fuse());
    /// assert!(!future.is_terminated());
    /// ```
    pub fn terminated() -> Self {
        Self { inner: None }
    }
}

impl<Fut: Future> FusedFuture for Fuse<Fut> {
    fn is_terminated(&self) -> bool {
        self.inner.is_none()
    }
}

impl<Fut: Future> Future for Fuse<Fut> {
    type Output = Fut::Output;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Fut::Output> {
        match self.as_mut().project().inner.as_pin_mut() {
            Some(fut) => fut.poll(cx).map(|output| {
                self.project().inner.set(None);
                output
            }),
            None => Poll::Pending,
        }
    }
}
