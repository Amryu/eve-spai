//! `spai-fit-bake <eve-online-static-data-latest-jsonl.zip> <out dir>`: bakes EVE's static data
//! into the fitting blob the apps load, plus a manifest naming its build and checksum.

use std::path::PathBuf;

use anyhow::{bail, Context as _, Result};
use sha2::Digest as _;

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let [_, zip, out] = args.as_slice() else {
        bail!("usage: spai-fit-bake <sde-jsonl.zip> <out dir>");
    };
    let bytes = std::fs::read(zip).with_context(|| format!("reading {zip}"))?;
    let dogma = spai_fit::bake::from_zip(&bytes)?;
    let blob = spai_fit::dogma::blob::encode(&dogma)?;
    let out = PathBuf::from(out);
    std::fs::create_dir_all(&out)?;
    let name = spai_fit::dogma::blob::file_name();
    let part = out.join(format!(".{name}.part"));
    std::fs::write(&part, &blob)?;
    std::fs::rename(&part, out.join(&name))?;
    let sha = format!("{:x}", sha2::Sha256::digest(&blob));
    let manifest = serde_json::json!({
        "format": spai_fit::dogma::blob::FORMAT,
        "build": dogma.sde_build,
        "file": name,
        "bytes": blob.len(),
        "sha256": sha,
    });
    std::fs::write(out.join(spai_fit::dogma::blob::manifest_name()), serde_json::to_string_pretty(&manifest)?)?;
    println!(
        "build {}: {} types, {} effects, {} attributes, {} buffs, {} traits -> {} bytes",
        dogma.sde_build,
        dogma.types.len(),
        dogma.effects.len(),
        dogma.attributes.len(),
        dogma.buffs.len(),
        dogma.traits.len(),
        blob.len()
    );
    Ok(())
}
