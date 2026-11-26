mod admin;
mod browser;
mod comments;
mod common;
mod contracts;
mod dates;
mod editor;
mod markdown;
mod models;
mod public;
mod select;
use blog_interface::prelude::*;
/// Mount the management or public page for this document.
///
/// # Errors
/// Returns the browser error if the page cannot be mounted.
#[wasm_bindgen::prelude::wasm_bindgen(start)]
pub fn start() -> Result<(), wasm_bindgen::JsValue> {
    browser::install_document_policies();
    let management = common::pathname().starts_with("/admin");
    let app = move || {
        if management {
            View::new(admin::Admin()).into_any()
        } else {
            View::new(public::Public()).into_any()
        }
    };
    blog_interface::mount_server("root", app, include_str!("corner_surfaces.txt"))
}
