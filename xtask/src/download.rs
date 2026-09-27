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

pub(crate) fn download_dictionary(dest: &Path) -> Result<(), Error> {
    expect_error("failed to download the dictionary", || {
        let sh = Shell::new()?;
        // The archive is ~18 MB; naming the cache by checksum makes a URL bump
        // download the new one instead of failing on the old file.
        let src = sh
            .create_dir(".cache")?
            .join(format!("libchewing-data-{}.zip", &DICTIONARY_SHA256[..16]));
        if !src.exists() {
            cmd!(sh, "curl -fL -o {src} {DICTIONARY_URL}").run()?;
        }

        let data = std::fs::read(&src)?;
        let digest: String = Sha256::digest(&data)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        if digest != DICTIONARY_SHA256 {
            // Likely an interrupted download; drop it so the next run starts over.
            sh.remove_path(&src)?;
            Err(format!(
                "checksum mismatch: expected {DICTIONARY_SHA256}, got {digest}"
            ))?;
        }

        sh.create_dir(dest)?;
        unzip(dest, Cursor::new(data))?;
        Ok(())
    })
}
