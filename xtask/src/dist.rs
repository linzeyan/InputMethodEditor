// SPDX-License-Identifier: GPL-3.0-or-later

use std::path::PathBuf;
use std::str::FromStr;

use scoped_error::{Error, expect_error};
use xshell::{Shell, cmd};

use crate::download::download_dictionary;
use crate::flags::Dist;

/// Keep in sync with `chewing_tip_core::PRODUCT_NAME`: tsfreg looks for
/// `<PRODUCT_NAME>.ico` beside itself.
const PRODUCT_NAME: &str = "InputMethodEditor";
/// Keep in sync with `chewing_tip_core::SETTINGS_EXE`: tsfreg registers it.
const SETTINGS_EXE: &str = "InputMethodEditorSettings.exe";

#[derive(Debug)]
pub(super) enum Target {
    Gnu,
    GnuLlvm,
    Msvc,
}

impl FromStr for Target {
    type Err = Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        expect_error("Failed to parse target", || match s {
            "gnu" => Ok(Target::Gnu),
            "gnullvm" => Ok(Target::GnuLlvm),
            "msvc" => Ok(Target::Msvc),
            _ => Err(format!("unknown target: {s}"))?,
        })
    }
}

pub(crate) fn dist(flags: Dist) -> Result<(), Error> {
    expect_error("Failed to build the package", || {
        if flags.msi && flags.arm64 {
            Err("wixl can't build an ARM64 MSI")?;
        }
        let sh = Shell::new()?;

        let release = flags.release.then_some("--release");
        let nightly = if flags.nightly {
            vec!["--features", "nightly"]
        } else {
            vec![]
        };
        let (x64_target, x86_target) = match flags.target {
            Some(Target::Gnu) => ("x86_64-pc-windows-gnu", "i686-pc-windows-gnu"),
            Some(Target::GnuLlvm) => ("x86_64-pc-windows-gnullvm", "i686-pc-windows-gnullvm"),
            None | Some(Target::Msvc) => ("x86_64-pc-windows-msvc", "i686-pc-windows-msvc"),
        };
        // ARM64 replaces x64 rather than joining it: both load the DLL from
        // the same 64-bit registry view, and only an ARM64X DLL could serve
        // both, which the gnullvm toolchain can't link.
        let (native_target, native_dir, package) = if flags.arm64 {
            (
                x64_target.replacen("x86_64", "aarch64", 1),
                "arm64",
                format!("{PRODUCT_NAME}-arm64"),
            )
        } else {
            (x64_target.to_owned(), "x64", PRODUCT_NAME.to_owned())
        };
        let profile = if flags.release { "release" } else { "debug" };
        let native_out = PathBuf::from("target").join(&native_target).join(profile);
        let x86_out = PathBuf::from("target").join(x86_target).join(profile);

        sh.set_var("RUSTFLAGS", "-Ctarget-feature=+crt-static");
        if matches!(flags.target, Some(Target::GnuLlvm)) {
            sh.set_var("RC", "llvm-rc");
        }
        cmd!(
            sh,
            "cargo build -p chewing_tip {release...} --target {native_target}"
        )
        .run()?;
        cmd!(
            sh,
            "cargo build -p tsfreg {release...} {nightly...} --target {native_target}"
        )
        .run()?;
        cmd!(
            sh,
            "cargo build -p chewing_tip {release...} --target {x86_target}"
        )
        .run()?;
        // The settings app embeds its web front end, so that is built first.
        {
            let _app = sh.push_dir("apps/settings");
            cmd!(sh, "npm ci").run()?;
            cmd!(sh, "npm run build").run()?;
        }
        // Without custom-protocol, Tauri loads the page from a dev server.
        cmd!(
            sh,
            "cargo build -p ime-settings {release...} --target {native_target} --features tauri/custom-protocol"
        )
        .run()?;

        let dir = PathBuf::from("dist").join(package);
        sh.remove_path(&dir)?;
        sh.copy_file(
            native_out.join("chewing_tip.dll"),
            sh.create_dir(dir.join(native_dir))?,
        )?;
        sh.copy_file(
            x86_out.join("chewing_tip.dll"),
            sh.create_dir(dir.join("x86"))?,
        )?;
        sh.copy_file(native_out.join("tsfreg.exe"), &dir)?;
        // Beside the DLL too, which tsfreg registers it from. On gnu targets
        // the WebView2 loader is a DLL, which its build script puts there.
        for file in [SETTINGS_EXE, "WebView2Loader.dll"] {
            sh.copy_file(native_out.join(file), dir.join(native_dir))?;
        }
        // The release profile keeps debuginfo for crash analysis; with MSVC it
        // goes to a separate .pdb, but gnullvm embeds it in the binaries.
        if flags.release && matches!(flags.target, Some(Target::GnuLlvm)) {
            let binaries = [
                dir.join(native_dir).join("chewing_tip.dll"),
                dir.join(native_dir).join(SETTINGS_EXE),
                dir.join("x86").join("chewing_tip.dll"),
                dir.join("tsfreg.exe"),
            ];
            cmd!(sh, "llvm-strip {binaries...}").run()?;
        }
        sh.copy_file(
            "tip/rc/im.chewing.Chewing.ico",
            dir.join(format!("{PRODUCT_NAME}.ico")),
        )?;
        sh.copy_file("COPYING.txt", &dir)?;
        download_dictionary(&dir.join("Dictionary"))?;

        if flags.msi {
            let wxs = sh.current_dir().join("installer/InputMethodEditor.wxs");
            let msi = sh.current_dir().join(dir.with_extension("msi"));
            let version = env!("CARGO_PKG_VERSION");
            // The .wxs names its files relative to the package folder.
            let _package = sh.push_dir(&dir);
            cmd!(sh, "wixl -a x64 -D Version={version} -o {msi} {wxs}").run()?;
            eprintln!("Wrote {}", msi.display());
        }
        Ok(())
    })
}
