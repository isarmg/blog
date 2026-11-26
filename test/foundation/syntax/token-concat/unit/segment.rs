use super::environment_error_message;
use std::{env::VarError, ffi::OsString, os::unix::ffi::OsStringExt};

#[test]
fn environment_diagnostics_report_the_failure_without_the_value() {
    assert_eq!(
        environment_error_message("PRIVATE_TEST_VALUE", &VarError::NotPresent),
        "environment variable \"PRIVATE_TEST_VALUE\" is not set"
    );
    let secret = OsString::from_vec(b"review-sentinel\xff".to_vec());
    assert_eq!(
        environment_error_message("PRIVATE_TEST_VALUE", &VarError::NotUnicode(secret)),
        "environment variable \"PRIVATE_TEST_VALUE\" is not valid Unicode"
    );
}

#[test]
fn camel_case_modes_preserve_case_and_underscore_rules() {
    use super::{CamelCase, camel_case};

    assert_eq!(camel_case("__aria__LABEL__", CamelCase::Upper), "AriaLabel");
    assert_eq!(camel_case("__aria__LABEL__", CamelCase::Lower), "ariaLabel");
    assert_eq!(
        camel_case("__aria__LABEL__", CamelCase::Edge),
        "__Aria_Label_"
    );
}
