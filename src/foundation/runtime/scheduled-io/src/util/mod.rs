mod maybe_dangling;
#[cfg(feature = "io")]
mod poll_buf;

pub(crate) use maybe_dangling::MaybeDangling;
#[cfg(feature = "io")]
pub(crate) use poll_buf::poll_read_buf;
