use std::path::PathBuf;

use windows::Foundation::Uri;
use windows::System::Launcher;

use scoped_error::expect_error;
use scoped_error::impl_context_error;

use crate::PRODUCT_NAME;

/// Per-user data folder, kept apart from upstream chewing's so both can be
/// installed side by side. libchewing creates it on first use.
pub fn user_dir() -> Result<PathBuf, ShellError> {
    expect_error("Unable to determine user dir", || {
        Ok(PathBuf::from(std::env::var("AppData")?).join(PRODUCT_NAME))
    })
}

pub fn open_url(url: &str) {
    if let Ok(uri) = Uri::CreateUri(&url.into()) {
        let _ = Launcher::LaunchUriAsync(&uri);
    }
}

impl_context_error!(pub ShellError);
