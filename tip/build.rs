// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) 2026 Kan-Ru Chen

use std::error::Error;

fn main() -> Result<(), Box<dyn Error>> {
    // The compiler lookup prints rerun-if-env-changed, which stops Cargo
    // from rerunning this on edits in the package, so the resources, with
    // resource.h and the icons they include, are named here.
    println!("cargo:rerun-if-changed=rc");
    // Required: without a resource compiler the DLL still links, but has no
    // menus, icons or strings, and a later build with one reuses that result.
    embed_resource::compile_for_everything("rc/ChewingTextService.rc", embed_resource::NONE)
        .manifest_required()?;
    Ok(())
}
