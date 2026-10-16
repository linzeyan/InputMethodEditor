// SPDX-FileCopyrightText: 2025-2026 Chewing Project Authors
//
// SPDX-License-Identifier: GPL-3.0-or-later

use std::error::Error;
use std::path::Path;
use std::{env, fs};

fn main() -> Result<(), Box<dyn Error>> {
    let config = slint_build::CompilerConfiguration::new().with_style("fluent".into());
    slint_build::compile_with_config("ui/settings.slint", config)?;

    // Written here for the version, which is the workspace's: the IME's.
    let dir = env::var("CARGO_MANIFEST_DIR")?;
    let path = |file: &str| {
        Path::new(&dir)
            .join(file)
            .display()
            .to_string()
            .replace('\\', "/")
    };
    let version = env::var("CARGO_PKG_VERSION")?;
    let numbers = version.replace('.', ",");
    let rc = format!(
        r#"#pragma code_page(65001)
1 ICON "{icon}"
1 24 "{manifest}"
1 VERSIONINFO
 FILEVERSION {numbers},0
 PRODUCTVERSION {numbers},0
 FILEOS 0x40004L
 FILETYPE 0x1L
BEGIN
    BLOCK "StringFileInfo"
    BEGIN
        BLOCK "040404b0"
        BEGIN
            VALUE "CompanyName", "InputMethodEditor"
            VALUE "FileDescription", "InputMethodEditor 設定"
            VALUE "FileVersion", "{version}"
            VALUE "OriginalFilename", "InputMethodEditorSettings.exe"
            VALUE "ProductName", "InputMethodEditor"
            VALUE "ProductVersion", "{version}"
        END
    END
    BLOCK "VarFileInfo"
    BEGIN
        VALUE "Translation", 0x404, 1200
    END
END
"#,
        // The IME's own, which the installer shows too.
        icon = path("../../tip/rc/im.chewing.Chewing.ico"),
        manifest = path("settings.manifest"),
    );
    let out = Path::new(&env::var("OUT_DIR")?).join("settings.rc");
    fs::write(&out, rc)?;
    println!("cargo:rerun-if-changed=settings.manifest");
    println!("cargo:rerun-if-changed=../../tip/rc/im.chewing.Chewing.ico");
    // Required: without it the exe still links, with no icon or manifest.
    embed_resource::compile(&out, embed_resource::NONE).manifest_required()?;
    Ok(())
}
