//! Linux event operations.

pub mod epoll;
mod eventfd;
mod pause;
mod poll;
mod select;

pub use crate::timespec::{Nsecs, Secs, Timespec};
pub use eventfd::{eventfd, EventfdFlags};
pub use pause::*;
pub use poll::{poll, PollFd, PollFlags};
pub use select::*;
