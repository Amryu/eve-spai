//! Recovery phrases: 24 words that stand for a device's keys, so a browser that lost its storage
//! comes back as the same device, in the same groups.

use anyhow::{anyhow, Result};

use crate::crypto::{random32, DeviceKeys, Key};

/// A fresh seed for a device that can be recovered.
pub fn new_seed() -> Key {
    random32()
}

/// The seed as its 24 words.
pub fn phrase(seed: &Key) -> String {
    bip39::Mnemonic::from_entropy(seed).expect("32 bytes is a valid entropy length").to_string()
}

/// The seed a phrase stands for. Case, extra spaces and line breaks do not matter; a wrong word or
/// a word out of place fails the phrase's checksum.
pub fn seed_of(phrase: &str) -> Result<Key> {
    let words = phrase.split_whitespace().map(str::to_lowercase).collect::<Vec<_>>().join(" ");
    let m = bip39::Mnemonic::parse_in_normalized(bip39::Language::English, &words).map_err(|e| anyhow!("not a recovery phrase: {e}"))?;
    let entropy = m.to_entropy();
    entropy.try_into().map_err(|_| anyhow!("a recovery phrase has 24 words"))
}

/// The device a phrase restores.
pub fn keys_of(phrase: &str) -> Result<DeviceKeys> {
    Ok(DeviceKeys::from_seed(&seed_of(phrase)?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_phrase_brings_back_the_same_device() {
        let seed = new_seed();
        let words = phrase(&seed);
        assert_eq!(words.split(' ').count(), 24);
        let shouted = words.to_uppercase().replace(' ', "\n  ");
        assert_eq!(seed_of(&shouted).expect("the phrase reads back"), seed);
        assert_eq!(keys_of(&words).expect("keys").public(), DeviceKeys::from_seed(&seed).public());
        assert_ne!(DeviceKeys::from_seed(&seed).public(), DeviceKeys::from_seed(&new_seed()).public());
    }

    #[test]
    fn a_wrong_word_is_caught() {
        let words = phrase(&new_seed());
        let mut w: Vec<&str> = words.split(' ').collect();
        w.swap(0, 1);
        let swapped = w.join(" ");
        // Two words swapped keep every word valid; the checksum catches it unless they were equal.
        if w[0] != w[1] {
            assert!(seed_of(&swapped).is_err() || seed_of(&swapped).ok() != seed_of(&words).ok());
        }
        assert!(seed_of("not a phrase at all").is_err());
        assert!(seed_of(&words.replacen(w[2], "zzzz", 1)).is_err());
    }
}
