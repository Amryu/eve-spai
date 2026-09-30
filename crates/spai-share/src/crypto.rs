//! The primitives shared wormhole groups are built on. The server only ever sees what these
//! produce: sealed records, keys wrapped to a member's public key, signatures.
//!
//! - Records: ChaCha20-Poly1305 under the group key, random 96-bit nonce, context as AAD.
//! - Key wrap: ephemeral X25519 to the recipient's static key, HKDF-SHA256, then a seal.
//! - Signatures: Ed25519 by the author's device key.
//! - Invites: a secret only the link carries; HKDF of it seals the invite, HMAC of it binds the
//!   joiner's public keys so the server cannot swap them.

use anyhow::{anyhow, Result};
use chacha20poly1305::aead::{Aead as _, KeyInit as _, Payload};
use chacha20poly1305::ChaCha20Poly1305;
use ed25519_dalek::pkcs8::{DecodePrivateKey as _, EncodePrivateKey as _, KeypairBytes, PublicKeyBytes};
use ed25519_dalek::{Signer as _, SigningKey, VerifyingKey};
use hmac::Mac as _;
use serde::{Deserialize, Serialize};
use sha2::Digest as _;

const NONCE_LEN: usize = 12;
type HmacSha256 = hmac::Hmac<sha2::Sha256>;

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

/// `nonce || ciphertext || tag`. `context` is authenticated, not stored: opening needs the same.
pub fn seal(key: &Key, context: &[u8], plain: &[u8]) -> Vec<u8> {
    let mut nonce = [0u8; NONCE_LEN];
    getrandom::getrandom(&mut nonce).expect("the OS random source");
    let buf = ChaCha20Poly1305::new(key.into())
        .encrypt(&nonce.into(), Payload { msg: plain, aad: context })
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
    ChaCha20Poly1305::new(key.into())
        .decrypt(nonce.into(), Payload { msg: rest, aad: context })
        .map_err(|_| anyhow!("does not open with this key"))
}

fn hkdf(salt: &[u8], ikm: &[u8], info: &[u8]) -> Key {
    let mut out = [0u8; 32];
    hkdf::Hkdf::<sha2::Sha256>::new(Some(salt), ikm).expand(info, &mut out).expect("32 bytes is a valid HKDF length");
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
        self.digest()[..8].chunks(2).map(|c| format!("{:02X}{:02X}", c[0], c[1])).collect::<Vec<_>>().join("-")
    }

    /// Names one device among a member's: anyone holding the keys can work it out again.
    pub fn device_id(&self) -> String {
        self.digest()[..16].iter().map(|b| format!("{b:02x}")).collect()
    }

    fn digest(&self) -> Vec<u8> {
        let mut h = sha2::Sha256::new();
        h.update(self.sign);
        h.update(self.enc);
        h.finalize().to_vec()
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
    sign: SigningKey,
    enc: x25519_dalek::StaticSecret,
}

#[derive(Serialize, Deserialize)]
struct StoredKeys {
    sign_pkcs8: String,
    enc: String,
}

impl DeviceKeys {
    pub fn generate() -> Self {
        let sign = SigningKey::from_bytes(&random32());
        // PKCS#8 v2, public key included: the form ring wrote and reads.
        let pair = KeypairBytes { secret_key: sign.to_bytes(), public_key: Some(PublicKeyBytes(sign.verifying_key().to_bytes())) };
        let pkcs8 = pair.to_pkcs8_der().expect("an Ed25519 key encodes").as_bytes().to_vec();
        Self::from_parts(pkcs8, random32()).expect("a fresh key parses")
    }

    fn from_parts(sign_pkcs8: Vec<u8>, enc: Key) -> Result<Self> {
        let pair = KeypairBytes::from_pkcs8_der(&sign_pkcs8).map_err(|_| anyhow!("bad signing key"))?;
        let sign = SigningKey::from_bytes(&pair.secret_key);
        if pair.public_key.is_some_and(|p| p.0 != sign.verifying_key().to_bytes()) {
            return Err(anyhow!("bad signing key: its public half does not match"));
        }
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
        PublicKeys { sign: self.sign.verifying_key().to_bytes(), enc: x25519_dalek::PublicKey::from(&self.enc).to_bytes() }
    }

    pub fn sign(&self, msg: &[u8]) -> Vec<u8> {
        self.sign.sign(msg).to_bytes().to_vec()
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
    let (Ok(key), Ok(sig)) = (VerifyingKey::from_bytes(sign_pub), ed25519_dalek::Signature::from_slice(sig)) else { return false };
    key.verify_strict(msg, &sig).is_ok()
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
fn invite_hmac(secret: &Key, msg: &[u8]) -> HmacSha256 {
    let mut m = <HmacSha256 as hmac::Mac>::new_from_slice(&hkdf(b"eve-spai invite", secret, b"mac")).expect("HMAC takes any key length");
    m.update(msg);
    m
}

pub fn invite_mac(secret: &Key, msg: &[u8]) -> Vec<u8> {
    invite_hmac(secret, msg).finalize().into_bytes().to_vec()
}

pub fn invite_mac_ok(secret: &Key, msg: &[u8], mac: &[u8]) -> bool {
    invite_hmac(secret, msg).verify_slice(mac).is_ok()
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

    /// ring, which the app used before, against this implementation, both ways: stored keys, signatures,
    /// sealed records, key derivation, MACs and fingerprints must not change.
    mod against_ring {
        use super::*;
        use ring::signature::KeyPair as _;

        fn ring_seal(key: &Key, context: &[u8], plain: &[u8]) -> Vec<u8> {
            use ring::aead::{Aad, LessSafeKey, Nonce, UnboundKey, CHACHA20_POLY1305};
            let nonce: [u8; 12] = random32()[..12].try_into().unwrap();
            let mut buf = plain.to_vec();
            LessSafeKey::new(UnboundKey::new(&CHACHA20_POLY1305, key).unwrap())
                .seal_in_place_append_tag(Nonce::assume_unique_for_key(nonce), Aad::from(context), &mut buf)
                .unwrap();
            [nonce.to_vec(), buf].concat()
        }

        fn ring_open(key: &Key, context: &[u8], sealed: &[u8]) -> Option<Vec<u8>> {
            use ring::aead::{Aad, LessSafeKey, Nonce, UnboundKey, CHACHA20_POLY1305};
            let (nonce, rest) = sealed.split_at(12);
            let mut buf = rest.to_vec();
            let k = LessSafeKey::new(UnboundKey::new(&CHACHA20_POLY1305, key).unwrap());
            k.open_in_place(Nonce::try_assume_unique_for_key(nonce).ok()?, Aad::from(context), &mut buf).ok().map(|p| p.to_vec())
        }

        fn ring_hkdf(salt: &[u8], ikm: &[u8], info: &[u8]) -> Key {
            struct Len;
            impl ring::hkdf::KeyType for Len {
                fn len(&self) -> usize {
                    32
                }
            }
            let mut out = [0u8; 32];
            ring::hkdf::Salt::new(ring::hkdf::HKDF_SHA256, salt).extract(ikm).expand(&[info], Len).unwrap().fill(&mut out).unwrap();
            out
        }

        #[test]
        fn a_key_ring_stored_loads_and_signs_the_same_bytes() {
            let pkcs8 = ring::signature::Ed25519KeyPair::generate_pkcs8(&ring::rand::SystemRandom::new()).unwrap();
            let theirs = ring::signature::Ed25519KeyPair::from_pkcs8(pkcs8.as_ref()).unwrap();
            let text = serde_json::json!({ "sign_pkcs8": b64(pkcs8.as_ref()), "enc": b64(&random32()) }).to_string();
            let ours = DeviceKeys::from_text(&text).unwrap();
            assert_eq!(ours.public().sign.as_slice(), theirs.public_key().as_ref());
            assert_eq!(ours.sign(b"op"), theirs.sign(b"op").as_ref(), "Ed25519 is deterministic");
            let back: serde_json::Value = serde_json::from_str(&ours.to_text()).unwrap();
            assert_eq!(back, serde_json::from_str::<serde_json::Value>(&text).unwrap(), "stored back unchanged");
        }

        #[test]
        fn a_key_made_here_loads_in_ring() {
            let ours = DeviceKeys::generate();
            let stored: serde_json::Value = serde_json::from_str(&ours.to_text()).unwrap();
            let pkcs8 = unb64(stored["sign_pkcs8"].as_str().unwrap()).unwrap();
            let theirs = ring::signature::Ed25519KeyPair::from_pkcs8(&pkcs8).expect("ring reads it");
            assert_eq!(ours.public().sign.as_slice(), theirs.public_key().as_ref());
            let sig = theirs.sign(b"op");
            assert!(verify(&ours.public().sign, b"op", sig.as_ref()));
            let ring_pub = ring::signature::UnparsedPublicKey::new(&ring::signature::ED25519, ours.public().sign);
            assert!(ring_pub.verify(b"op", &ours.sign(b"op")).is_ok());
        }

        #[test]
        fn records_sealed_by_either_open_in_the_other() {
            let k = random32();
            assert_eq!(ring_open(&k, b"g1", &seal(&k, b"g1", b"hole")).unwrap(), b"hole");
            assert_eq!(open(&k, b"g1", &ring_seal(&k, b"g1", b"hole")).unwrap(), b"hole");
            assert!(open(&k, b"g2", &ring_seal(&k, b"g1", b"hole")).is_err());
        }

        #[test]
        fn derived_keys_macs_and_fingerprints_match() {
            let (salt, ikm, info) = (random32(), random32(), b"eve-spai share key wrap g1/e0");
            assert_eq!(hkdf(&salt, &ikm, info), ring_hkdf(&salt, &ikm, info));
            let secret = random32();
            let mk = ring::hmac::Key::new(ring::hmac::HMAC_SHA256, &ring_hkdf(b"eve-spai invite", &secret, b"mac"));
            assert_eq!(invite_mac(&secret, b"keys"), ring::hmac::sign(&mk, b"keys").as_ref());
            assert_eq!(invite_key(&secret), ring_hkdf(b"eve-spai invite", &secret, b"seal"));
            let p = DeviceKeys::generate().public();
            let d = ring::digest::digest(&ring::digest::SHA256, &[p.sign, p.enc].concat());
            assert_eq!(p.device_id(), d.as_ref()[..16].iter().map(|b| format!("{b:02x}")).collect::<String>());
        }
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

/// Values the ring-based versions produced, frozen: the same bytes must come out on every target,
/// the browser included, where ring is not there to compare against. Test keys, never used for real.
#[cfg(test)]
mod vectors {
    use super::*;

    const KEYS: &str = r#"{"sign_pkcs8":"MFECAQEwBQYDK2VwBCIEIPoMGf62rO4TWKitISRIz_aRY7TKQWbR6kaTINQYhAFZgSEARqIfI4KBseQso4_9goerdFvLnat2ke1CCsBx6fpBO-U","enc":"BQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQU"}"#;

    #[test]
    fn stored_keys_sign_and_name_themselves_as_before() {
        let k = DeviceKeys::from_text(KEYS).unwrap();
        assert_eq!(b64(&k.sign(b"eve-spai vector")), "Pdo8jCHisepcTA5oSWzpvdKoWCioTxm5Vl-_Oh_vsI3eIDS2g-VJJ5QwLo73lF4COAUcXSlc9IiMqYVM4Kf2Bg");
        assert_eq!(
            serde_json::to_string(&k.public()).unwrap(),
            r#"{"sign":"RqIfI4KBseQso4_9goerdFvLnat2ke1CCsBx6fpBO-U","enc":"UKYUCbHd0DJemxa3AOcZ6XcsBwALG9d4bpB8ZT0gSV0"}"#
        );
        assert_eq!(k.public().fingerprint(), "1BD4-7861-2C13-521E");
        assert_eq!(k.public().device_id(), "1bd478612c13521e699409408fd97ae2");
    }

    #[test]
    fn invites_records_and_wrapped_keys_read_as_before() {
        assert_eq!(b64(&invite_key(&[1; 32])), "4Bg4TyeJTPqU7b9Ux-NUEKU2V9k_9vmTQtQD0UDL848");
        assert_eq!(b64(&invite_mac(&[1; 32], b"keys")), "F42YmuPnGxTlTZ23etx1_85qtrNoQvDzAedtoTC6Tu4");
        assert_eq!(open(&[2; 32], b"g1/e0", &unb64("CQkJCQkJCQkJCQkJehh8qOFdhy4OuqIAD1dJP6kxolY").unwrap()).unwrap(), b"hole");
        let w: Wrapped = serde_json::from_str(
            r#"{"eph":"uS4jtcONWKbsiDmeYBED9k6weHvU8m10C9fZhyEnR10","sealed":"a6gSxMv9EUR37SxDOCPiADSnsBtdInJpB6XadgViQWyIYcnyQRRnscQ4ZdHFHOsmkKDaebCSEjCyfJNX"}"#,
        )
        .unwrap();
        assert_eq!(DeviceKeys::from_text(KEYS).unwrap().unwrap_key(&w, b"g1/e0").unwrap(), [3; 32]);
    }
}
