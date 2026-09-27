use std::fs;
use std::path::{Path, PathBuf};
use std::ptr::null_mut;

use windows::Foundation::Uri;
use windows::System::Launcher;
use windows::Win32::Foundation::{HLOCAL, LocalFree};
use windows::Win32::Security::Authorization::{
    ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1, SE_FILE_OBJECT,
    SetNamedSecurityInfoW,
};
use windows::Win32::Security::{
    GetSecurityDescriptorSacl, LABEL_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR,
};
use windows::Win32::Storage::FileSystem::{DELETE, FILE_GENERIC_READ, FILE_GENERIC_WRITE};
use windows::core::{HSTRING, PCWSTR, w};

use scoped_error::expect_error;
use scoped_error::impl_context_error;

use crate::PRODUCT_NAME;
use crate::config::grant_app_container_access;

/// Per-user data folder, kept apart from upstream chewing's so both can be
/// installed side by side. libchewing creates it on first use.
pub fn user_dir() -> Result<PathBuf, ShellError> {
    expect_error("Unable to determine user dir", || {
        Ok(PathBuf::from(std::env::var("AppData")?).join(PRODUCT_NAME))
    })
}

/// Lets AppContainer processes (Start menu search, Store apps) use the user
/// dictionaries. They run at low integrity, so the ALL APPLICATION PACKAGES
/// entry only lets them read; writing what they learn also needs a low label,
/// as low integrity can't write up to the default medium one.
pub fn share_user_dir(dir: &Path) -> Result<(), ShellError> {
    expect_error("Unable to share user dir with AppContainer apps", || {
        fs::create_dir_all(dir)?;
        let path = HSTRING::from(dir.as_os_str());
        let path = PCWSTR(path.as_ptr());
        // DELETE because chewing saves by renaming a temp file over the old one.
        let access = FILE_GENERIC_READ | FILE_GENERIC_WRITE | DELETE;
        grant_app_container_access(path, SE_FILE_OBJECT, access.0)?;
        unsafe {
            let mut sd = PSECURITY_DESCRIPTOR::default();
            // Low mandatory label, no write up, inherited by files and folders.
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                w!("S:(ML;OICI;NW;;;LW)"),
                SDDL_REVISION_1,
                &mut sd,
                None,
            )?;
            let mut sacl = null_mut();
            let result = GetSecurityDescriptorSacl(
                sd,
                &mut Default::default(),
                &mut sacl,
                &mut Default::default(),
            )
            .and_then(|()| {
                SetNamedSecurityInfoW(
                    path,
                    SE_FILE_OBJECT,
                    LABEL_SECURITY_INFORMATION,
                    None,
                    None,
                    None,
                    Some(sacl),
                )
                .ok()
            });
            LocalFree(Some(HLOCAL(sd.0)));
            result?;
        }
        Ok(())
    })
}

pub fn open_url(url: &str) {
    if let Ok(uri) = Uri::CreateUri(&url.into()) {
        let _ = Launcher::LaunchUriAsync(&uri);
    }
}

impl_context_error!(pub ShellError);
