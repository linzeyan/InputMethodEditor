use std::io::Cursor;
use std::path::Path;

use scoped_error::{Error, expect_error};
use sha2::{Digest, Sha256};
use xshell::{Shell, cmd};

use crate::zip::unzip;

const DICTIONARY_URL: &str = "https://codeberg.org/chewing/libchewing-data/releases/download/v2026.9.25/libchewing-data-2026.9.25-Generic.zip";
// Pinned after checking the release's OpenPGP signature against the libchewing
// signing key (release@chewing.im). Re-verify the signature when bumping the URL.
const DICTIONARY_SHA256: &str = "6ca66e008c4f60de689a3b61fd6dcb428dbf87922573fd84f946bf478ce925a8";

const UNIHAN_URL: &str = "https://www.unicode.org/Public/18.0.0/ucd/Unihan.zip";
// Unicode signs nothing; pinned from a download over HTTPS.
const UNIHAN_SHA256: &str = "4c93ea9c1f636451729a840978f1667a53886af37ba854fdcce109721c63d43e";

pub(crate) fn download_dictionary(dest: &Path) -> Result<(), Error> {
    expect_error("failed to download the dictionary", || {
        let data = fetch("libchewing-data", DICTIONARY_URL, DICTIONARY_SHA256)?;
        Shell::new()?.create_dir(dest)?;
        unzip(dest, Cursor::new(data))?;
        Ok(())
    })
}

/// The Unihan database's zip.
pub(crate) fn download_unihan() -> Result<Vec<u8>, Error> {
    fetch("Unihan", UNIHAN_URL, UNIHAN_SHA256)
}

fn fetch(name: &str, url: &str, sha256: &str) -> Result<Vec<u8>, Error> {
    expect_error(format!("failed to download {url}"), || {
        let sh = Shell::new()?;
        // Naming the cache by checksum makes a URL bump download the new file
        // instead of failing on the old one.
        let src = sh
            .create_dir(".cache")?
            .join(format!("{name}-{}.zip", &sha256[..16]));
        if !src.exists() {
            cmd!(sh, "curl -fL -o {src} {url}").run()?;
        }

        let data = std::fs::read(&src)?;
        let digest: String = Sha256::digest(&data)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        if digest != sha256 {
            // Likely an interrupted download; drop it so the next run starts over.
            sh.remove_path(&src)?;
            Err(format!(
                "checksum mismatch: expected {sha256}, got {digest}"
            ))?;
        }
        Ok(data)
    })
}
