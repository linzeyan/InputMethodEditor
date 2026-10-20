// SPDX-License-Identifier: GPL-3.0-or-later

use std::fs::File;
use std::io::Write;

use scoped_error::{Error, expect_error};

use super::flags::UpdateVersion;

pub(super) fn update_version(flags: UpdateVersion) -> Result<(), Error> {
    expect_error("Failed to update version", || {
        let yy = flags.major;
        let mm = flags.minor;
        let rv = flags.patch;
        let bn = flags.build.unwrap_or_default();
        let mut version_rc = File::create("tip/rc/version.rc")?;
        indoc::writedoc!(
            version_rc,
            r#"
            #define VER_FILEVERSION             {yy},{mm},{rv},{bn}
            #define VER_FILEVERSION_STR         "{yy}.{mm}.{rv}.{bn}\0"
            #define VER_PRODUCTVERSION          {yy},{mm},{rv},{bn}
            #define VER_PRODUCTVERSION_STR      "{yy}.{mm}.{rv}.{bn}\0"
        "#
        )?;
        Ok(())
    })
}
