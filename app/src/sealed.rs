//! A fallback for the refresh token when the OS keychain cannot be used.
//!
//! The keychain stays the primary store and is always tried first. A machine with no Secret Service
//! provider (no GNOME Keyring, KWallet or KeePassXC, ordinary on a minimal Linux desktop) would
//! otherwise have nowhere to keep the token, and every ESI feature would be unusable.
//!
//! # What this protects, and what it does not
//!
//! The file is encrypted with ChaCha20-Poly1305 under a key derived from facts about the OS account
//! it belongs to, so the file alone is worthless: copied to another machine, another user, a backup,
//! a synced folder or a disk image, it will not open.
//!
//! It does **not** protect against code running as that user. Any key the app can derive unattended,
//! a program running as the same account can derive too, and an unlocked OS keychain has the same
//! ceiling. What it buys over plaintext is that the token does not leak by being copied somewhere.
//!
//! Not plaintext, and never a fallback the user did not need: writing here is only ever reached
//! after the keychain has actually failed, and [`promote`] moves the token back the moment a
//! keychain appears.

use anyhow::{Context as _, Result, anyhow};
use base64::Engine as _;
use ring::aead::{Aad, LessSafeKey, NONCE_LEN, Nonce, UnboundKey};
use std::collections::BTreeMap;
use std::io::Write as _;
use std::path::PathBuf;
use std::sync::Mutex;

const VERSION: u32 = 1;
const FILE: &str = "tokens.sealed";
/// PBKDF2 rounds. The binding is not a secret, so this is not the load-bearing part of the design;
/// it is what an attacker who has the file but must guess the account it came from has to pay per
/// guess. Cheap enough to run once at startup.
const ROUNDS: u32 = 200_000;

/// Serialised as JSON because the file is tiny and a human looking at it should be able to see that
/// it holds ciphertext and nothing else.
#[derive(serde::Serialize, serde::Deserialize, Default)]
struct Vault {
    v: u32,
    /// Not secret. It makes the derived key unique per install, so the same account on two machines
    /// does not produce the same key, and it defeats any precomputation against common accounts.
    salt: String,
    /// Character id to sealed token.
    items: BTreeMap<String, Item>,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct Item {
    n: String,
    c: String,
}

/// One writer at a time. Several characters can refresh concurrently, and the file is rewritten
/// whole.
static LOCK: Mutex<()> = Mutex::new(());

fn b64() -> base64::engine::general_purpose::GeneralPurpose {
    base64::engine::general_purpose::STANDARD
}

#[cfg(test)]
thread_local! {
    /// Per-thread, not `EVE_SPAI_DATA_DIR`: that variable is process-wide and every other test in
    /// this binary reads it concurrently, so a test that set it would quietly move another test's
    /// profile out from under it.
    static TEST_DIR: std::cell::RefCell<Option<PathBuf>> = const { std::cell::RefCell::new(None) };
    /// Same reason, for the account the key is derived from.
    static TEST_USER: std::cell::RefCell<Option<String>> = const { std::cell::RefCell::new(None) };
}

fn path() -> Result<PathBuf> {
    #[cfg(test)]
    if let Some(d) = TEST_DIR.with(|d| d.borrow().clone()) {
        return Ok(d.join(FILE));
    }
    Ok(crate::store::data_dir()?.join(FILE))
}

/// What ties the key to this account on this machine.
///
/// Every part is something the OS knows about the account rather than something the app invents, so
/// it survives a reinstall and does not survive being moved. Missing parts are simply absent: a
/// machine with no hostname still gets a key, just a less specific one, and the salt carries the
/// uniqueness regardless.
fn binding() -> Vec<u8> {
    let mut out = Vec::new();
    let mut push = |label: &str, v: &str| {
        out.extend_from_slice(label.as_bytes());
        out.push(b'=');
        out.extend_from_slice(v.as_bytes());
        out.push(0);
    };
    #[cfg(test)]
    let forced = TEST_USER.with(|u| u.borrow().clone());
    #[cfg(not(test))]
    let forced: Option<String> = None;
    if let Some(u) = forced {
        push("user", &u);
    } else {
        for key in ["USER", "USERNAME", "LOGNAME"] {
            if let Ok(v) = std::env::var(key) {
                push("user", &v);
                break;
            }
        }
    }
    for key in ["HOME", "USERPROFILE"] {
        if let Ok(v) = std::env::var(key) {
            push("home", &v);
            break;
        }
    }
    if let Some(h) = hostname() {
        push("host", &h);
    }
    if let Some(m) = machine_id() {
        push("machine", &m);
    }
    out
}

fn hostname() -> Option<String> {
    if let Ok(v) = std::env::var("COMPUTERNAME") {
        return Some(v);
    }
    // Portable enough: Linux and macOS both expose it here, and where they do not the other parts
    // of the binding still apply.
    std::fs::read_to_string("/proc/sys/kernel/hostname")
        .ok()
        .or_else(|| std::fs::read_to_string("/etc/hostname").ok())
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
}

/// The one part that is genuinely machine-specific where it exists. Linux only by design: reading
/// the Windows registry or macOS IOKit for the equivalent would be platform code this cannot
/// compile-check, and the account parts already carry the binding.
fn machine_id() -> Option<String> {
    ["/etc/machine-id", "/var/lib/dbus/machine-id"]
        .iter()
        .find_map(|p| std::fs::read_to_string(p).ok())
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
}

fn derive_key(salt: &[u8]) -> [u8; 32] {
    let mut key = [0u8; 32];
    ring::pbkdf2::derive(
        ring::pbkdf2::PBKDF2_HMAC_SHA256,
        std::num::NonZeroU32::new(ROUNDS).expect("non-zero"),
        salt,
        &binding(),
        &mut key,
    );
    key
}

fn load_vault() -> Result<Vault> {
    let p = path()?;
    let Ok(raw) = std::fs::read_to_string(&p) else {
        return Ok(Vault { v: VERSION, salt: String::new(), items: BTreeMap::new() });
    };
    let v: Vault = serde_json::from_str(&raw).context("parsing the sealed token file")?;
    if v.v != VERSION {
        return Err(anyhow!("sealed token file is version {}, expected {VERSION}", v.v));
    }
    Ok(v)
}

fn write_vault(v: &Vault) -> Result<()> {
    let p = path()?;
    if let Some(dir) = p.parent() {
        std::fs::create_dir_all(dir).ok();
    }
    let json = serde_json::to_string(v)?;
    // Written to a temp file and renamed, so a crash mid-write cannot leave a half-file that loses
    // every character's token at once.
    let tmp = p.with_extension("sealed.tmp");
    {
        let mut f = std::fs::File::create(&tmp)?;
        restrict(&f)?;
        f.write_all(json.as_bytes())?;
        f.sync_all()?;
    }
    std::fs::rename(&tmp, &p)?;
    Ok(())
}

/// Owner-only. Encryption is what actually protects the contents; this keeps a second user on the
/// same machine from getting as far as having the ciphertext to attack.
#[cfg(unix)]
fn restrict(f: &std::fs::File) -> Result<()> {
    use std::os::unix::fs::PermissionsExt as _;
    f.set_permissions(std::fs::Permissions::from_mode(0o600))?;
    Ok(())
}

/// Windows puts the data directory under the user's own profile, which is ACL'd to that user by
/// default, so there is no per-file step that adds anything here.
#[cfg(not(unix))]
fn restrict(_f: &std::fs::File) -> Result<()> {
    Ok(())
}

fn key_for(v: &mut Vault) -> Result<LessSafeKey> {
    if v.salt.is_empty() {
        let mut salt = [0u8; 32];
        getrandom::getrandom(&mut salt).map_err(|e| anyhow!("rng failure: {e}"))?;
        v.salt = b64().encode(salt);
    }
    let salt = b64().decode(&v.salt).context("decoding the salt")?;
    let bytes = derive_key(&salt);
    let k = UnboundKey::new(&ring::aead::CHACHA20_POLY1305, &bytes)
        .map_err(|_| anyhow!("building the encryption key"))?;
    Ok(LessSafeKey::new(k))
}

/// The character id is authenticated alongside the token, so a sealed blob cannot be moved from one
/// character to another inside the file.
fn aad(character_id: i64) -> Vec<u8> {
    format!("eve-spai:refresh:{character_id}").into_bytes()
}

pub fn save(character_id: i64, refresh_token: &str) -> Result<()> {
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut v = load_vault()?;
    let key = key_for(&mut v)?;
    let mut nonce = [0u8; NONCE_LEN];
    getrandom::getrandom(&mut nonce).map_err(|e| anyhow!("rng failure: {e}"))?;
    let mut buf = refresh_token.as_bytes().to_vec();
    key.seal_in_place_append_tag(
        Nonce::assume_unique_for_key(nonce),
        Aad::from(aad(character_id)),
        &mut buf,
    )
    .map_err(|_| anyhow!("sealing the refresh token"))?;
    v.items.insert(
        character_id.to_string(),
        Item { n: b64().encode(nonce), c: b64().encode(&buf) },
    );
    write_vault(&v)?;
    note_in_use(true);
    Ok(())
}

/// `Ok(None)` when there is nothing sealed for this character. An error means there is something
/// and it would not open, which is worth telling apart: it is what a moved or tampered file looks
/// like.
pub fn load(character_id: i64) -> Result<Option<String>> {
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut v = load_vault()?;
    let Some(item) = v.items.get(&character_id.to_string()) else {
        return Ok(None);
    };
    let nonce_bytes: [u8; NONCE_LEN] = b64()
        .decode(&item.n)
        .ok()
        .and_then(|b| b.try_into().ok())
        .ok_or_else(|| anyhow!("the sealed token's nonce is malformed"))?;
    let mut buf = b64().decode(&item.c).context("decoding the sealed token")?;
    let key = key_for(&mut v)?;
    let plain = key
        .open_in_place(
            Nonce::assume_unique_for_key(nonce_bytes),
            Aad::from(aad(character_id)),
            &mut buf,
        )
        .map_err(|_| {
            anyhow!(
                "the saved login could not be decrypted. It is tied to this machine and this user \
                 account, so a copied profile directory will not open here."
            )
        })?;
    Ok(Some(String::from_utf8(plain.to_vec()).context("the sealed token is not text")?))
}

pub fn delete(character_id: i64) -> Result<()> {
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut v = load_vault()?;
    if v.items.remove(&character_id.to_string()).is_none() {
        return Ok(());
    }
    if v.items.is_empty() {
        // Nothing left to hold, so the file goes rather than lingering as an empty vault.
        let _ = std::fs::remove_file(path()?);
        note_in_use(false);
        return Ok(());
    }
    write_vault(&v)
}

/// Whether anything is being kept here. The user is told, because "your token is not in the OS
/// keychain" is something they are entitled to know without reading a log.
///
/// Cached, because the settings view asks once per frame and the answer changes only when this
/// module writes.
static IN_USE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static IN_USE_KNOWN: std::sync::Once = std::sync::Once::new();

pub fn in_use() -> bool {
    // Tests use a per-thread vault directory, which a process-wide cache cannot represent. The
    // cache is an optimisation for the running app, so tests simply ask the file.
    #[cfg(test)]
    return peek();
    #[cfg(not(test))]
    {
        IN_USE_KNOWN.call_once(|| IN_USE.store(peek(), std::sync::atomic::Ordering::Relaxed));
        IN_USE.load(std::sync::atomic::Ordering::Relaxed)
    }
}

fn peek() -> bool {
    load_vault().map(|v| !v.items.is_empty()).unwrap_or(false)
}

fn note_in_use(v: bool) {
    IN_USE.store(v, std::sync::atomic::Ordering::Relaxed);
    IN_USE_KNOWN.call_once(|| {});
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Its own directory per test, set on this thread only, so these can run beside everything else
    /// in the binary without touching a variable the rest of it reads.
    fn scratch(name: &str) {
        let dir = std::env::temp_dir().join(format!("eve-spai-sealed-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("scratch dir");
        TEST_DIR.with(|d| *d.borrow_mut() = Some(dir));
    }

    fn as_user(name: &str) {
        TEST_USER.with(|u| *u.borrow_mut() = Some(name.to_owned()));
    }

    #[test]
    fn a_sealed_token_comes_back_the_same() {
        scratch("roundtrip");
        assert!(!in_use(), "nothing sealed to begin with");
        save(90_000_001, "refresh-token-value").expect("seal");
        assert!(in_use());
        assert_eq!(load(90_000_001).expect("open"), Some("refresh-token-value".to_owned()));
        // A character with nothing sealed is absent, not an error: that is how the caller tells
        // "never logged in" from "would not open".
        assert_eq!(load(90_000_002).expect("no entry"), None);
    }

    /// The whole point of the file: it must be ciphertext, not an encoding. A reader who opens it
    /// must not find the token by looking.
    #[test]
    fn the_file_never_contains_the_token() {
        scratch("opaque");
        let secret = "this-must-not-appear-anywhere";
        save(90_000_003, secret).expect("seal");
        let raw = std::fs::read(path().expect("path")).expect("read");
        assert!(
            !raw.windows(secret.len()).any(|w| w == secret.as_bytes()),
            "the token is in the file in the clear"
        );
        // Nor base64 of it, which would be an encoding pretending to be encryption.
        let encoded = b64().encode(secret);
        assert!(
            !raw.windows(encoded.len()).any(|w| w == encoded.as_bytes()),
            "the token is in the file, merely encoded"
        );
    }

    /// Copied to another account, the file is worthless.
    #[test]
    fn a_vault_from_another_account_does_not_open() {
        scratch("binding");
        as_user("the-owner");
        save(90_000_004, "refresh-token-value").expect("seal");
        assert_eq!(load(90_000_004).expect("own account"), Some("refresh-token-value".to_owned()));

        // Same file, same machine, different account.
        as_user("somebody-else");
        assert!(load(90_000_004).is_err(), "another account must not be able to open it");
    }

    /// The character id is authenticated, so a sealed blob cannot be moved onto another character
    /// inside the file to make the app hand that character's session someone else's token.
    #[test]
    fn a_blob_cannot_be_moved_between_characters() {
        scratch("aad");
        save(90_000_005, "alpha-token").expect("seal");
        let mut v = load_vault().expect("vault");
        let item = v.items.remove("90000005").expect("sealed alpha");
        v.items.insert("90000006".to_owned(), item);
        write_vault(&v).expect("write");
        assert!(load(90_000_006).is_err(), "the id it was sealed for is part of the seal");
    }

    /// Tampering is rejected rather than silently producing rubbish, which is what an AEAD is for.
    #[test]
    fn a_flipped_bit_is_refused() {
        scratch("tamper");
        save(90_000_007, "refresh-token-value").expect("seal");
        let mut v = load_vault().expect("vault");
        let item = v.items.get_mut("90000007").expect("sealed");
        let mut raw = b64().decode(&item.c).expect("decode");
        raw[0] ^= 0x01;
        item.c = b64().encode(&raw);
        write_vault(&v).expect("write");
        assert!(load(90_000_007).is_err());
    }

    /// Forgetting a character has to actually remove it, and the last one takes the file with it
    /// rather than leaving an empty vault that still reports itself as in use.
    #[test]
    fn deleting_the_last_token_removes_the_file() {
        scratch("delete");
        save(90_000_008, "a").expect("seal");
        save(90_000_009, "b").expect("seal");
        delete(90_000_008).expect("delete");
        assert_eq!(load(90_000_008).expect("gone"), None);
        assert_eq!(load(90_000_009).expect("kept"), Some("b".to_owned()));
        delete(90_000_009).expect("delete");
        assert!(!in_use());
        assert!(!path().expect("path").exists(), "an empty vault is not left behind");
    }

    /// Two saves of the same value must not produce the same ciphertext: a repeated nonce is the
    /// classic way an AEAD stops protecting anything.
    #[test]
    fn every_seal_uses_a_fresh_nonce() {
        scratch("nonce");
        save(90_000_010, "same-value").expect("seal");
        let first = load_vault().expect("vault").items.remove("90000010").expect("sealed");
        save(90_000_010, "same-value").expect("seal again");
        let second = load_vault().expect("vault").items.remove("90000010").expect("sealed");
        assert_ne!(first.n, second.n, "nonce reused");
        assert_ne!(first.c, second.c);
    }

    /// Owner-only on the platforms that have file modes. The encryption is what protects the
    /// contents; this keeps another user on the box from having the ciphertext at all.
    #[cfg(unix)]
    #[test]
    fn the_file_is_owner_only() {
        use std::os::unix::fs::PermissionsExt as _;
        scratch("mode");
        save(90_000_011, "x").expect("seal");
        let mode = std::fs::metadata(path().expect("path")).expect("stat").permissions().mode();
        assert_eq!(mode & 0o077, 0, "group or other can read the vault: {mode:o}");
    }
}
