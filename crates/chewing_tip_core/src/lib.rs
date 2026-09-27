pub mod config;
pub mod shell;

/// Names the per-user registry key and data folder. Changing it orphans the
/// settings and learned phrases stored under the previous name.
pub const PRODUCT_NAME: &str = "InputMethodEditor";
