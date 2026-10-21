//! The dataset as one file: a short header the client can check before inflating, then the
//! gzip of its postcard encoding. Both apps load it; the desktop bakes it from EVE's zip, the
//! web fetches a prebaked copy.

use std::io::{Read as _, Write as _};

use anyhow::{bail, Context as _, Result};

use super::Dogma;

const MAGIC: &[u8; 7] = b"SPAIFIT";
/// Bumped whenever the encoded structures change, or what is baked does: 2 brought the weather
/// beacons, 3 the mutaplasmids; the file name carries it, so a client never keeps a copy made without them
/// and a cached file of the old name is never served in place of the new.
pub const FORMAT: u16 = 3;
const HEADER_LEN: usize = MAGIC.len() + 2 + 4;

pub fn file_name() -> String {
    format!("dogma-v{FORMAT}.bin.gz")
}

pub fn manifest_name() -> String {
    format!("dogma-v{FORMAT}.json")
}

/// `(format, sde_build)` from the header, checking the magic and the format.
pub fn header(bytes: &[u8]) -> Result<(u16, u32)> {
    if bytes.len() < HEADER_LEN || &bytes[..MAGIC.len()] != MAGIC {
        bail!("not a fitting data blob");
    }
    let format = u16::from_le_bytes([bytes[7], bytes[8]]);
    if format != FORMAT {
        bail!("fitting data format {format}, this build reads {FORMAT}");
    }
    let build = u32::from_le_bytes([bytes[9], bytes[10], bytes[11], bytes[12]]);
    Ok((format, build))
}

pub fn encode(d: &Dogma) -> Result<Vec<u8>> {
    let body = postcard::to_allocvec(d).context("encoding fitting data")?;
    let mut out = Vec::with_capacity(body.len() / 3);
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&FORMAT.to_le_bytes());
    out.extend_from_slice(&d.sde_build.to_le_bytes());
    let mut gz = flate2::write::GzEncoder::new(out, flate2::Compression::best());
    gz.write_all(&body)?;
    Ok(gz.finish()?)
}

pub fn decode(bytes: &[u8]) -> Result<Dogma> {
    header(bytes)?;
    let mut body = Vec::new();
    flate2::read::GzDecoder::new(&bytes[HEADER_LEN..]).read_to_end(&mut body).context("inflating fitting data")?;
    let d: Dogma = postcard::from_bytes(&body).context("decoding fitting data")?;
    Ok(d.finish())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dogma::test_data;

    #[test]
    fn a_dataset_survives_the_round_trip_with_its_lookups() {
        let d = test_data::small();
        let bytes = encode(&d).unwrap();
        assert_eq!(header(&bytes).unwrap(), (FORMAT, 42));
        let back = decode(&bytes).unwrap();
        assert_eq!(back, d);
        assert_eq!(back.type_(587).unwrap().name, "Rifter");
        assert_eq!(back.variants_of(2048), &[2048]);
    }

    #[test]
    fn foreign_and_future_files_are_refused_before_inflating() {
        assert!(header(b"PK\x03\x04 not ours at all").is_err());
        let mut bytes = encode(&test_data::small()).unwrap();
        bytes[7] = 9;
        assert!(decode(&bytes).unwrap_err().to_string().contains("format 9"));
        assert!(decode(&bytes[..5]).is_err());
    }
}
