//! Generated Web API bindings retained for the Blog workspace.
//!
//! Cargo features gate browser types and their dependent bindings. The package
//! manifest lists the types used by the browser and interface crates.

#![doc(html_root_url = "https://docs.rs/web-sys/0.3")]
#![no_std]
#![allow(deprecated)]

extern crate alloc;

mod features;
#[allow(unused_imports)]
pub use features::*;

pub use js_sys;
pub use wasm_bindgen;

/// Getter for the `Window` object
///
/// [MDN Documentation]
///
/// *This API requires the following crate features to be activated: `Window`*
///
/// [MDN Documentation]: https://developer.mozilla.org/en-US/docs/Web/API/Window
#[cfg(feature = "Window")]
pub fn window() -> Option<Window> {
    use wasm_bindgen::JsCast;

    js_sys::global().dyn_into::<Window>().ok()
}
