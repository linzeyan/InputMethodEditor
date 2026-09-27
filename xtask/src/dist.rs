// SPDX-License-Identifier: GPL-3.0-or-later

use std::fs::File;
use std::io;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use scoped_error::{Error, expect_error};
use xshell::{Shell, cmd};
use zip::ZipWriter;
use zip::write::SimpleFileOptions;

use crate::download::download_dictionary;
use crate::flags::Dist;

/// Keep in sync with `chewing_tip_core::PRODUCT_NAME`: tsfreg looks for
/// `<PRODUCT_NAME>.ico` beside itself.
const PRODUCT_NAME: &str = "InputMethodEditor";

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

/// tsfreg writes HKLM, so the scripts ask for elevation; a double-clicked
/// batch file cannot request UAC on its own.
fn elevated_bat(verb: &str) -> String {
    format!(
        "@echo off\r\npowershell -NoProfile -Command \"Start-Process -FilePath '%~dp0tsfreg.exe' -ArgumentList '{verb}' -Verb RunAs\"\r\n"
    )
}

pub(crate) fn dist(flags: Dist) -> Result<(), Error> {
    expect_error("Failed to build the portable package", || {
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
        // The release profile keeps debuginfo for crash analysis; with MSVC it
        // goes to a separate .pdb, but gnullvm embeds it in the binaries.
        if flags.release && matches!(flags.target, Some(Target::GnuLlvm)) {
            let binaries = [
                dir.join(native_dir).join("chewing_tip.dll"),
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
        sh.write_file(dir.join("register.bat"), elevated_bat("register"))?;
        sh.write_file(dir.join("unregister.bat"), elevated_bat("unregister"))?;
        download_dictionary(&dir.join("Dictionary"))?;

        let zip_path = dir.with_extension("zip");
        zip_dir(&dir, &zip_path)?;
        eprintln!("Wrote {}", zip_path.display());

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

/// Entries have no top folder: Explorer's Extract All already creates one named
/// after the zip, and a second level inside it only gets in the way.
fn zip_dir(dir: &Path, dest: &Path) -> Result<(), Error> {
    expect_error("failed to write the zip file", || {
        let options = SimpleFileOptions::default();
        let mut zip = ZipWriter::new(File::create(dest)?);
        let mut pending = vec![dir.to_path_buf()];
        while let Some(current) = pending.pop() {
            for entry in std::fs::read_dir(&current)? {
                let path = entry?.path();
                if path.is_dir() {
                    pending.push(path);
                    continue;
                }
                let name = path.strip_prefix(dir)?.to_string_lossy().replace('\\', "/");
                zip.start_file(name, options)?;
                io::copy(&mut File::open(&path)?, &mut zip)?;
            }
        }
        zip.finish()?;
        Ok(())
    })
}
