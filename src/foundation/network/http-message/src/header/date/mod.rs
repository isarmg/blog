//! Parse and format timestamps from HTTP headers.
//!
//! Multiple HTTP header fields store timestamps.
//! For example a response created on May 15, 2015 may contain the header
//! `Date: Fri, 15 May 2015 15:34:21 GMT`. Since the timestamp does not
//! contain any timezone or leap second information it is equivalent to
//! writing 1431696861 Unix time. Rust’s `SystemTime` is used to store
//! these timestamps.
//!
//! Use [`parse_http_date`] to parse a date into a [`SystemTime`]. Use
//! [`HttpDate`] to format timestamps through its [`Display`] implementation,
//! which writes directly to the formatter.
#![forbid(unsafe_code)]

use std::error;
use std::fmt::{self, Display, Formatter};
use std::time::SystemTime;

pub use timestamp::HttpDate;

mod timestamp;

/// An opaque error type for all parsing errors.
#[derive(Debug)]
pub struct Error(());

impl error::Error for Error {}

impl Display for Error {
    fn fmt(&self, f: &mut Formatter) -> Result<(), fmt::Error> {
        f.write_str("string contains no or an invalid date")
    }
}

/// Parse a date from an HTTP header field.
///
/// Supports the preferred IMF-fixdate and the legacy RFC 850 and
/// asctime formats. Two digit years are mapped to dates between
/// 1970 and 2069.
pub fn parse_http_date(s: &str) -> Result<SystemTime, Error> {
    s.parse::<HttpDate>().map(|d| d.into())
}

#[path = "../../../../../../../test/foundation/network/http-message/unit/src/header/date/tests.rs"]
#[cfg(test)]
mod tests;
