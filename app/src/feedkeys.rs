//! The user's own identity for signed and encrypted outside feeds: an Ed25519 key that signs
//! requests and an X25519 key that opens libsodium sealed-box answers.
//!
//! Separate from the wormhole-sharing device keys on purpose, so either can be replaced without
//! breaking the other and a signature made for one protocol is never one the other accepts. Kept
//! in the OS keychain, or sealed to this machine when there is none. The private halves are never
//! printed, logged or written anywhere else.

use anyhow::{anyhow, Result};
use base64::Engine as _;
use sha2::{Digest, Sha256};

const SERVICE: &str = "eve-spai-feeds";
const ACCOUNT: &str = "identity";
/// The sealed-file slot. Character ids are positive and the sharing keys use 0.
const SEALED_SLOT: i64 = -1;
pub const CLI_FLAG: &str = "--feed-keys";

pub struct FeedKeys {
    sign: ed25519_dalek::SigningKey,
    enc: x25519_dalek::StaticSecret,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct Stored {
    v: u32,
    ed25519: String,
    x25519: String,
}

fn b64() -> base64::engine::GeneralPurpose {
    base64::engine::general_purpose::STANDARD
}

fn random32() -> Result<[u8; 32]> {
    let mut b = [0u8; 32];
    getrandom::getrandom(&mut b).map_err(|e| anyhow!("no randomness: {e}"))?;
    Ok(b)
}

fn key32(s: &str) -> Result<[u8; 32]> {
    b64().decode(s).ok().and_then(|b| b.try_into().ok()).ok_or_else(|| anyhow!("the stored feed key is damaged"))
}

impl FeedKeys {
    fn generate() -> Result<Self> {
        Ok(Self { sign: ed25519_dalek::SigningKey::from_bytes(&random32()?), enc: x25519_dalek::StaticSecret::from(random32()?) })
    }

    fn to_text(&self) -> String {
        serde_json::to_string(&Stored { v: 1, ed25519: b64().encode(self.sign.to_bytes()), x25519: b64().encode(self.enc.to_bytes()) })
            .expect("plain struct serializes")
    }

    fn from_text(text: &str) -> Result<Self> {
        let s: Stored = serde_json::from_str(text).map_err(|_| anyhow!("the stored feed key is damaged"))?;
        Ok(Self { sign: ed25519_dalek::SigningKey::from_bytes(&key32(&s.ed25519)?), enc: x25519_dalek::StaticSecret::from(key32(&s.x25519)?) })
    }

    pub fn ed25519_pub(&self) -> [u8; 32] {
        self.sign.verifying_key().to_bytes()
    }

    pub fn x25519_pub(&self) -> [u8; 32] {
        x25519_dalek::PublicKey::from(&self.enc).to_bytes()
    }

    pub fn sign(&self, msg: &[u8]) -> [u8; 64] {
        use ed25519_dalek::Signer as _;
        self.sign.sign(msg).to_bytes()
    }

    /// libsodium `crypto_box_seal_open` with the X25519 key.
    pub fn open_sealed(&self, sealed: &[u8]) -> Result<Vec<u8>> {
        crypto_box::SecretKey::from(self.enc.to_bytes())
            .unseal(sealed)
            .map_err(|_| anyhow!("the answer could not be decrypted with this machine's feed key"))
    }

    #[cfg(test)]
    pub fn for_test(sign_seed: u8, enc_seed: u8) -> Self {
        Self { sign: ed25519_dalek::SigningKey::from_bytes(&[sign_seed; 32]), enc: x25519_dalek::StaticSecret::from([enc_seed; 32]) }
    }

    /// SHA-256 over both public keys, first 32 hex digits in groups of 4.
    pub fn fingerprint(&self) -> String {
        let mut h = Sha256::new();
        h.update(self.ed25519_pub());
        h.update(self.x25519_pub());
        let hex: String = h.finalize().iter().take(16).map(|b| format!("{b:02x}")).collect();
        hex.as_bytes().chunks(4).map(|c| std::str::from_utf8(c).expect("hex")).collect::<Vec<_>>().join(" ")
    }

    /// The block to hand a feed operator, under the first line the feed asks for. Public keys only.
    pub fn public_block(&self, label: &str) -> String {
        format!(
            "{label}\ned25519_pub: {}\nx25519_pub:  {}\nfingerprint: {}",
            b64().encode(self.ed25519_pub()),
            b64().encode(self.x25519_pub()),
            self.fingerprint()
        )
    }
}

fn keychain() -> Option<keyring::Entry> {
    keyring::Entry::new(SERVICE, ACCOUNT).ok()
}

fn stored() -> Option<String> {
    if let Some(text) = keychain().and_then(|e| e.get_password().ok()) {
        return Some(text);
    }
    crate::sealed::load(SEALED_SLOT).ok().flatten()
}

fn store(text: &str) -> Result<()> {
    if keychain().is_some_and(|e| e.set_password(text).is_ok()) {
        return Ok(());
    }
    crate::sealed::save(SEALED_SLOT, text)
}

/// Where the keys are, for telling the user.
pub fn location() -> String {
    if keychain().and_then(|e| e.get_password().ok()).is_some() {
        format!("OS keychain, service \"{SERVICE}\", account \"{ACCOUNT}\"")
    } else {
        "the encrypted tokens.sealed file in the EVE Spai data folder".to_owned()
    }
}

/// The keys, made on first use. A stored key that does not parse is an error, never replaced: the
/// feeds that know its public half would stop answering.
pub fn identity() -> Result<&'static FeedKeys> {
    static KEYS: std::sync::OnceLock<FeedKeys> = std::sync::OnceLock::new();
    if let Some(k) = KEYS.get() {
        return Ok(k);
    }
    let keys = match stored() {
        Some(t) => FeedKeys::from_text(&t)?,
        None => {
            let k = FeedKeys::generate()?;
            store(&k.to_text())?;
            k
        }
    };
    Ok(KEYS.get_or_init(|| keys))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_round_trip_and_the_block_holds_only_public_halves() {
        let k = FeedKeys::generate().unwrap();
        let back = FeedKeys::from_text(&k.to_text()).unwrap();
        assert_eq!((back.ed25519_pub(), back.x25519_pub()), (k.ed25519_pub(), k.x25519_pub()));
        let block = k.public_block("example-keys v1");
        assert!(block.starts_with("example-keys v1\n"));
        assert!(!block.contains(&b64().encode(k.sign.to_bytes())));
        assert!(!block.contains(&b64().encode(k.enc.to_bytes())));
        assert_ne!(k.ed25519_pub(), k.x25519_pub());
    }

    #[test]
    fn the_fingerprint_is_sha256_of_both_keys_in_groups_of_four() {
        let k = FeedKeys { sign: ed25519_dalek::SigningKey::from_bytes(&[1; 32]), enc: x25519_dalek::StaticSecret::from([2; 32]) };
        let mut cat = k.ed25519_pub().to_vec();
        cat.extend(k.x25519_pub());
        let hex: String = Sha256::digest(&cat).iter().map(|b| format!("{b:02x}")).collect();
        let fp = k.fingerprint();
        assert_eq!(fp.replace(' ', ""), hex[..32]);
        assert_eq!(fp.split(' ').count(), 8);
        assert!(fp.split(' ').all(|g| g.len() == 4));
    }

    #[test]
    fn a_damaged_stored_key_is_an_error() {
        assert!(FeedKeys::from_text("{\"v\":1,\"ed25519\":\"AAAA\",\"x25519\":\"AAAA\"}").is_err());
        assert!(FeedKeys::from_text("not json").is_err());
    }

    #[test]
    fn signatures_verify_against_the_published_key() {
        let k = FeedKeys::generate().unwrap();
        let sig = ed25519_dalek::Signature::from_bytes(&k.sign(b"GET /v1/intel"));
        let vk = ed25519_dalek::VerifyingKey::from_bytes(&k.ed25519_pub()).unwrap();
        assert!(vk.verify_strict(b"GET /v1/intel", &sig).is_ok());
    }
}
