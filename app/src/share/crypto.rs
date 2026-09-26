//! The primitives shared wormhole groups are built on. The server only ever sees what these
//! produce: sealed records, keys wrapped to a member's public key, signatures.
//!
//! - Records: ChaCha20-Poly1305 under the group key, random 96-bit nonce, context as AAD.
//! - Key wrap: ephemeral X25519 to the recipient's static key, HKDF-SHA256, then a seal.
//! - Signatures: Ed25519 by the author's device key.
//! - Invites: a secret only the link carries; HKDF of it seals the invite, HMAC of it binds the
//!   joiner's public keys so the server cannot swap them.

use anyhow::{anyhow, Result};
use ring::aead::{Aad, LessSafeKey, Nonce, UnboundKey, CHACHA20_POLY1305, NONCE_LEN};
use ring::signature::{Ed25519KeyPair, KeyPair as _, UnparsedPublicKey, ED25519};
use serde::{Deserialize, Serialize};

pub type Key = [u8; 32];

pub fn random32() -> Key {
    let mut k = [0u8; 32];
    getrandom::getrandom(&mut k).expect("the OS random source");
    k
}

pub fn b64(bytes: &[u8]) -> String {
    use base64::Engine as _;
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

pub fn unb64(s: &str) -> Result<Vec<u8>> {
    use base64::Engine as _;
    base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(s.trim()).map_err(|e| anyhow!("bad base64: {e}"))
}

pub fn unb64_32(s: &str) -> Result<Key> {
    unb64(s)?.try_into().map_err(|_| anyhow!("expected 32 bytes"))
}

fn aead(key: &Key) -> LessSafeKey {
    LessSafeKey::new(UnboundKey::new(&CHACHA20_POLY1305, key).expect("a 32-byte key"))
}

/// `nonce || ciphertext || tag`. `context` is authenticated, not stored: opening needs the same.
pub fn seal(key: &Key, context: &[u8], plain: &[u8]) -> Vec<u8> {
    let mut nonce = [0u8; NONCE_LEN];
    getrandom::getrandom(&mut nonce).expect("the OS random source");
    let mut buf = plain.to_vec();
    aead(key)
        .seal_in_place_append_tag(Nonce::assume_unique_for_key(nonce), Aad::from(context), &mut buf)
        .expect("sealing never fails for in-memory data");
    let mut out = nonce.to_vec();
    out.extend(buf);
    out
}

pub fn open(key: &Key, context: &[u8], sealed: &[u8]) -> Result<Vec<u8>> {
    if sealed.len() < NONCE_LEN {
        return Err(anyhow!("sealed data too short"));
    }
    let (nonce, rest) = sealed.split_at(NONCE_LEN);
    let nonce = Nonce::try_assume_unique_for_key(nonce).map_err(|_| anyhow!("bad nonce"))?;
    let mut buf = rest.to_vec();
    let plain = aead(key)
        .open_in_place(nonce, Aad::from(context), &mut buf)
        .map_err(|_| anyhow!("does not open with this key"))?;
    Ok(plain.to_vec())
}

fn hkdf(salt: &[u8], ikm: &[u8], info: &[u8]) -> Key {
    struct Len;
    impl ring::hkdf::KeyType for Len {
        fn len(&self) -> usize {
            32
        }
    }
    let prk = ring::hkdf::Salt::new(ring::hkdf::HKDF_SHA256, salt).extract(ikm);
    let mut out = [0u8; 32];
    prk.expand(&[info], Len).expect("32 bytes is a valid HKDF length").fill(&mut out).expect("fill");
    out
}

/// A device's public half, as published inside the group.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublicKeys {
    #[serde(with = "key_b64")]
    pub sign: Key,
    #[serde(with = "key_b64")]
    pub enc: Key,
}

impl PublicKeys {
    /// Short, comparable over voice: the first bytes of both keys' hash.
    pub fn fingerprint(&self) -> String {
        let mut h = ring::digest::Context::new(&ring::digest::SHA256);
        h.update(&self.sign);
        h.update(&self.enc);
        let d = h.finish();
        d.as_ref()[..8].chunks(2).map(|c| format!("{:02X}{:02X}", c[0], c[1])).collect::<Vec<_>>().join("-")
    }
}

mod key_b64 {
    pub fn serialize<S: serde::Serializer>(k: &super::Key, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&super::b64(k))
    }
    pub fn deserialize<'de, D: serde::Deserializer<'de>>(d: D) -> Result<super::Key, D::Error> {
        let s = <String as serde::Deserialize>::deserialize(d)?;
        super::unb64_32(&s).map_err(serde::de::Error::custom)
    }
}

/// This install's own keys. The private halves never leave the machine.
pub struct DeviceKeys {
    sign_pkcs8: Vec<u8>,
    sign: Ed25519KeyPair,
    enc: x25519_dalek::StaticSecret,
}

#[derive(Serialize, Deserialize)]
struct StoredKeys {
    sign_pkcs8: String,
    enc: String,
}

impl DeviceKeys {
    pub fn generate() -> Self {
        let rng = ring::rand::SystemRandom::new();
        let pkcs8 = Ed25519KeyPair::generate_pkcs8(&rng).expect("the OS random source");
        Self::from_parts(pkcs8.as_ref().to_vec(), random32()).expect("a fresh key parses")
    }

    fn from_parts(sign_pkcs8: Vec<u8>, enc: Key) -> Result<Self> {
        let sign = Ed25519KeyPair::from_pkcs8(&sign_pkcs8).map_err(|_| anyhow!("bad signing key"))?;
        Ok(DeviceKeys { sign_pkcs8, sign, enc: x25519_dalek::StaticSecret::from(enc) })
    }

    pub fn to_text(&self) -> String {
        serde_json::to_string(&StoredKeys { sign_pkcs8: b64(&self.sign_pkcs8), enc: b64(&self.enc.to_bytes()) })
            .expect("plain struct serializes")
    }

    pub fn from_text(text: &str) -> Result<Self> {
        let s: StoredKeys = serde_json::from_str(text)?;
        Self::from_parts(unb64(&s.sign_pkcs8)?, unb64_32(&s.enc)?)
    }

    pub fn public(&self) -> PublicKeys {
        let sign: Key = self.sign.public_key().as_ref().try_into().expect("Ed25519 public keys are 32 bytes");
        PublicKeys { sign, enc: x25519_dalek::PublicKey::from(&self.enc).to_bytes() }
    }

    pub fn sign(&self, msg: &[u8]) -> Vec<u8> {
        self.sign.sign(msg).as_ref().to_vec()
    }

    /// Opens a key wrapped to this device by [`wrap_key`].
    pub fn unwrap_key(&self, w: &Wrapped, context: &[u8]) -> Result<Key> {
        let eph = x25519_dalek::PublicKey::from(w.eph);
        let shared = self.enc.diffie_hellman(&eph);
        let k = wrap_kdf(&w.eph, &self.public().enc, shared.as_bytes(), context);
        open(&k, context, &w.sealed)?.try_into().map_err(|_| anyhow!("wrapped key is not 32 bytes"))
    }
}

pub fn verify(sign_pub: &Key, msg: &[u8], sig: &[u8]) -> bool {
    UnparsedPublicKey::new(&ED25519, sign_pub).verify(msg, sig).is_ok()
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Wrapped {
    #[serde(with = "key_b64")]
    pub eph: Key,
    #[serde(with = "vec_b64")]
    pub sealed: Vec<u8>,
}

mod vec_b64 {
    pub fn serialize<S: serde::Serializer>(k: &[u8], s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&super::b64(k))
    }
    pub fn deserialize<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Vec<u8>, D::Error> {
        let s = <String as serde::Deserialize>::deserialize(d)?;
        super::unb64(&s).map_err(serde::de::Error::custom)
    }
}

fn wrap_kdf(eph: &Key, recipient: &Key, shared: &[u8], context: &[u8]) -> Key {
    let mut salt = eph.to_vec();
    salt.extend_from_slice(recipient);
    let mut info = b"eve-spai share key wrap ".to_vec();
    info.extend_from_slice(context);
    hkdf(&salt, shared, &info)
}

/// `key` sealed so only the holder of `recipient`'s private key opens it.
pub fn wrap_key(recipient: &Key, key: &Key, context: &[u8]) -> Wrapped {
    let eph_secret = x25519_dalek::StaticSecret::from(random32());
    let eph = x25519_dalek::PublicKey::from(&eph_secret).to_bytes();
    let shared = eph_secret.diffie_hellman(&x25519_dalek::PublicKey::from(*recipient));
    let k = wrap_kdf(&eph, recipient, shared.as_bytes(), context);
    Wrapped { eph, sealed: seal(&k, context, key) }
}

/// The key an invite is sealed with, from the secret in the link.
pub fn invite_key(secret: &Key) -> Key {
    hkdf(b"eve-spai invite", secret, b"seal")
}

/// Binds a joiner's public keys to the invite secret, so an admin can tell they were not swapped.
pub fn invite_mac(secret: &Key, msg: &[u8]) -> Vec<u8> {
    let k = ring::hmac::Key::new(ring::hmac::HMAC_SHA256, &hkdf(b"eve-spai invite", secret, b"mac"));
    ring::hmac::sign(&k, msg).as_ref().to_vec()
}

pub fn invite_mac_ok(secret: &Key, msg: &[u8], mac: &[u8]) -> bool {
    let k = ring::hmac::Key::new(ring::hmac::HMAC_SHA256, &hkdf(b"eve-spai invite", secret, b"mac"));
    ring::hmac::verify(&k, msg, mac).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_sealed_record_opens_only_with_its_key_and_context() {
        let k = random32();
        let s = seal(&k, b"group 1", b"hole");
        assert_eq!(open(&k, b"group 1", &s).unwrap(), b"hole");
        assert!(open(&random32(), b"group 1", &s).is_err());
        assert!(open(&k, b"group 2", &s).is_err(), "moved to another group");
        let mut bad = s.clone();
        *bad.last_mut().unwrap() ^= 1;
        assert!(open(&k, b"group 1", &bad).is_err(), "tampered");
    }

    #[test]
    fn a_wrapped_key_opens_for_its_recipient_only() {
        let (alice, eve) = (DeviceKeys::generate(), DeviceKeys::generate());
        let gk = random32();
        let w = wrap_key(&alice.public().enc, &gk, b"g1/e0");
        assert_eq!(alice.unwrap_key(&w, b"g1/e0").unwrap(), gk);
        assert!(eve.unwrap_key(&w, b"g1/e0").is_err());
        assert!(alice.unwrap_key(&w, b"g1/e1").is_err(), "another epoch's context");
    }

    #[test]
    fn signatures_bind_author_and_message() {
        let a = DeviceKeys::generate();
        let sig = a.sign(b"op");
        assert!(verify(&a.public().sign, b"op", &sig));
        assert!(!verify(&a.public().sign, b"op2", &sig));
        assert!(!verify(&DeviceKeys::generate().public().sign, b"op", &sig));
    }

    #[test]
    fn device_keys_survive_a_round_trip_through_storage() {
        let a = DeviceKeys::generate();
        let b = DeviceKeys::from_text(&a.to_text()).unwrap();
        assert_eq!(a.public(), b.public());
    }

    #[test]
    fn the_invite_mac_rejects_swapped_keys() {
        let secret = random32();
        let joiner = serde_json::to_vec(&DeviceKeys::generate().public()).unwrap();
        let mac = invite_mac(&secret, &joiner);
        assert!(invite_mac_ok(&secret, &joiner, &mac));
        let swapped = serde_json::to_vec(&DeviceKeys::generate().public()).unwrap();
        assert!(!invite_mac_ok(&secret, &swapped, &mac), "the server put its own keys in");
        assert!(!invite_mac_ok(&random32(), &joiner, &mac), "someone without the link");
    }
}
