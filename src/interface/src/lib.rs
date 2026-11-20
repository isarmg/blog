#![forbid(unsafe_code)]
//! Shared browser-only support. The Linux server crate remains independent.
pub mod api;
pub mod components;
pub mod contracts;
pub mod i18n;
pub mod modal;
pub mod server_corners;
pub mod theme;

extern crate self as blog_interface;

pub use blog_dom::*;
pub use wasm_bindgen;
pub use web_sys;

use wasm_bindgen::{JsCast, JsValue};

#[cfg(target_arch = "wasm32")]
thread_local! {
    // Keep the controller alive for this document; its pagehide listener releases
    // observers, styles and URLs before an uncached page leaves.
    static SERVER_CORNERS: std::cell::RefCell<Option<server_corners::ServerCornersController>> = const { std::cell::RefCell::new(None) };
}

/// Mount the application and decorate the product's explicit surface roles.
/// The selector includes all candidate roles, including currently hidden surfaces.
pub fn mount_server<F, V>(root_id: &str, app: F, surfaces: &'static str) -> Result<(), JsValue>
where
    F: FnOnce() -> V + 'static,
    V: IntoView + 'static,
{
    let document = web_sys::window()
        .and_then(|window| window.document())
        .ok_or_else(|| JsValue::from_str("Browser document is unavailable"))?;
    let html = document
        .document_element()
        .ok_or_else(|| JsValue::from_str("Browser document root is missing"))?;
    // A timed-out startup cannot later replace its visible failure page.
    if html.get_attribute("data-blog-startup").as_deref() == Some("failed") {
        return Err(JsValue::from_str("Application startup has already failed"));
    }
    let root = document
        .get_element_by_id(root_id)
        .ok_or_else(|| JsValue::from_str("Application root is missing"))?
        .dyn_into::<web_sys::HtmlElement>()?;
    blog_dom::browser_panic::set_once();
    i18n::initialize();
    theme::initialize();
    let mounted = blog_interface::mount::mount_to(root, app);
    html.set_attribute("data-blog-appearance", "content-blocks")?;
    html.set_attribute("data-blog-selection", "underline")?;
    #[cfg(target_arch = "wasm32")]
    let controller = crate::server_corners::start(&document, surfaces)?;
    #[cfg(not(target_arch = "wasm32"))]
    let _ = surfaces;
    html.set_attribute("data-blog-runtime", "blog-interface")?;
    #[cfg(target_arch = "wasm32")]
    SERVER_CORNERS.with(|value| *value.borrow_mut() = Some(controller));
    mounted.forget();
    Ok(())
}
