//! Runtime utilities

#[cfg(feature = "adapter-tracing")]
pub mod tracing;
#[cfg(feature = "adapter-tracing")]
pub use self::tracing::{CurrentSpanExecutor, MkSpanExecutor, WithSpanExecutor};

#[cfg(feature = "tokio")]
pub mod tokio;

#[cfg(feature = "tokio")]
pub use self::tokio::{TokioExecutor, TokioIo, TokioTimer};
