#![allow(clippy::needless_doctest_main)]
#![warn(
    missing_debug_implementations,
    missing_docs,
    rust_2018_idioms,
    unreachable_pub
)]
#![doc(test(
    no_crate_inject,
    attr(deny(warnings, rust_2018_idioms), allow(dead_code, unused_variables))
))]
#![cfg_attr(docsrs, feature(doc_cfg))]

//! Runtime utilities for HTTP file streaming and graceful task shutdown.

mod loom;
mod util;

#[cfg(feature = "io")]
pub mod io;
pub mod sync;
#[cfg(feature = "rt")]
pub mod task;
