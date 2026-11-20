use super::{HeaderMap, ACCEPT_ENCODING};

/// Whether a request permits either of the supported content representations.
///
/// This records acceptability rather than preference. A responder can prefer
/// gzip whenever it is allowed, and use identity only when that is allowed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AcceptedEncodings {
    /// A representation carrying `Content-Encoding: gzip` is acceptable.
    pub gzip: bool,
    /// An unencoded representation is acceptable.
    pub identity: bool,
}

/// Determine whether gzip and identity representations are acceptable.
///
/// An absent or empty `Accept-Encoding` field permits identity only. Explicit
/// coding weights override `*`; identity is permitted unless explicitly
/// excluded, including by `*;q=0` when identity has no explicit weight.
/// Any zero weight on a repeated coding excludes that coding. Coding and
/// parameter names are case insensitive, and quality values use the HTTP
/// grammar (`0` or `1` with at most three fractional digits).
///
/// Unsupported codings are ignored. Invalid field text or malformed parameters
/// on gzip, identity or `*` reject both supported representations, so ambiguous
/// exclusions cannot accidentally permit a response the request forbids.
pub fn accepted_encodings(headers: &HeaderMap) -> AcceptedEncodings {
    let reject = AcceptedEncodings {
        gzip: false,
        identity: false,
    };
    let mut gzip = None;
    let mut identity = None;
    let mut wildcard = None;
    for field in headers.get_all(ACCEPT_ENCODING) {
        let Ok(value) = field.to_str() else {
            return reject;
        };
        for entry in value.split(',') {
            let (coding, parameter) = match entry.split_once(';') {
                Some((coding, parameter)) => (coding, Some(parameter)),
                None => (entry, None),
            };
            let coding = coding.trim();
            let target = if coding.eq_ignore_ascii_case("gzip") {
                &mut gzip
            } else if coding.eq_ignore_ascii_case("identity") {
                &mut identity
            } else if coding == "*" {
                &mut wildcard
            } else {
                continue;
            };
            let Some(allowed) = parameter.map_or(Some(true), allows_coding) else {
                return reject;
            };
            *target.get_or_insert(true) &= allowed;
        }
    }
    AcceptedEncodings {
        gzip: gzip.or(wildcard).unwrap_or(false),
        identity: identity.or(wildcard).unwrap_or(true),
    }
}

fn allows_coding(parameter: &str) -> Option<bool> {
    let (name, value) = parameter.split_once('=')?;
    if !name.trim().eq_ignore_ascii_case("q") {
        return None;
    }
    let value = value.trim();
    let (whole, fraction) = value.split_once('.').unwrap_or((value, ""));
    if fraction.len() > 3 {
        return None;
    }
    match whole {
        "1" if fraction.bytes().all(|byte| byte == b'0') => Some(true),
        "0" if fraction.bytes().all(|byte| byte.is_ascii_digit()) => {
            Some(fraction.bytes().any(|byte| byte != b'0'))
        }
        _ => None,
    }
}
