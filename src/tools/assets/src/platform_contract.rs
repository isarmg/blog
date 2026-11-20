#![forbid(unsafe_code)]
//! Build-time platform contract shared by the product entry points.
//!
//! This file is also a standalone Cargo build script. Keep it std-only so that
//! checking the platform never depends on compiling the asset builder itself.

use std::env;

pub const NATIVE_TARGET: &str = "x86_64-unknown-linux-gnu";
pub const BROWSER_TARGET: &str = "wasm32-unknown-unknown";

fn validate(host: &str, target: &str, package: &str) -> Result<(), String> {
    if host != NATIVE_TARGET {
        return Err(format!(
            "unsupported build host {host:?}; {package} requires {NATIVE_TARGET}"
        ));
    }
    // These libraries also compile natively for domain and kernel tests. All
    // other entry points and build tools are native-only; proc macros are built
    // for HOST even when their consumers target the browser.
    let browser = matches!(package, "blog-browser" | "blog-interface" | "blog-dom");
    if target != NATIVE_TARGET && !(browser && target == BROWSER_TARGET) {
        let supported = if browser {
            format!("{NATIVE_TARGET} (native tests) or {BROWSER_TARGET} (browser)")
        } else {
            NATIVE_TARGET.to_owned()
        };
        return Err(format!(
            "unsupported build target {target:?}; {package} supports only {supported}"
        ));
    }
    Ok(())
}

pub fn check() {
    // Cargo already tracks the build script's Rust sources and target. Limit
    // the fallback package scan to the package's own manifest.
    println!("cargo:rerun-if-changed=Cargo.toml");
    let host = env::var("HOST").expect("Cargo build host");
    let target = env::var("TARGET").expect("Cargo build target");
    let package = env::var("CARGO_PKG_NAME").expect("Cargo package name");
    validate(&host, &target, &package).unwrap_or_else(|error| panic!("{error}"));
}

#[allow(dead_code)] // Used directly as build.rs or included by an existing build script.
fn main() {
    check();
}

#[cfg(test)]
#[path = "../../../../test/tools/assets/unit/platform_contract.rs"]
mod tests;
