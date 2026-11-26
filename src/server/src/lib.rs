mod config;
mod config_definition;
mod content;
mod home;
mod lifecycle;
mod media;
mod schema;
mod schema_definition;
mod static_assets;

mod app;
mod database;
mod error;

pub use error::Error;
pub use static_assets::AssetError;

#[cfg(any(test, feature = "maintenance"))]
pub(crate) use app::now_micros;
pub(crate) use app::{ApiResult, AppError, AppState, absent, invalid, site_author_id};
pub use blog_application::server_cli::ContractJson;

const PRODUCT_ID: &str = "blog";

mod runtime;

/// Serve the configured instance at `root` until shutdown.
///
/// # Errors
/// Returns an error if the installation or database cannot be validated,
/// a listener cannot start, or the service cannot finish shutting down.
pub async fn run(root: &std::path::Path) -> Result<(), Error> {
    runtime::run(root).await
}

#[cfg(test)]
#[path = "../../../test/server/unit/tests.rs"]
mod tests;

#[cfg(any(test, feature = "maintenance"))]
mod administrator;

#[cfg(feature = "maintenance")]
pub mod maintenance;

/// Verify the identity compiled into this product before runtime or offline work.
/// This consumes canonical identity bytes and their digest in both executables.
///
/// # Errors
/// Returns an error if the embedded identity is malformed, fails its digest
/// check, or disagrees with the inputs compiled into this executable.
pub fn verify_compiled_identity() -> Result<(), Error> {
    static_assets::verify_identity()?;
    Ok(())
}
