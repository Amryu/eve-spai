//! domain / Unlicense), sorted (LC_ALL=C, i.e. byte order — matches `str` Ord for ASCII) and

use std::sync::LazyLock;

/// Gzip-compressed, byte-sorted, newline-separated lowercase word list.
static WORDS_GZ: &[u8] = include_bytes!("../assets/english_words.txt.gz");

static DICT: LazyLock<Box<[Box<str>]>> = LazyLock::new(load);

fn load() -> Box<[Box<str>]> {
    use std::io::Read;
    let mut text = String::new();
    if flate2::read::GzDecoder::new(WORDS_GZ).read_to_string(&mut text).is_err() {
        return Box::new([]);
    }
    text.lines().map(Box::<str>::from).collect()
}

pub fn is_word(w: &str) -> bool {
    if w.is_empty() {
        return false;
    }
    let lw = w.to_ascii_lowercase();
    DICT.binary_search_by(|entry| entry.as_ref().cmp(lw.as_str())).is_ok()
}

pub fn preload() {
    LazyLock::force(&DICT);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_words_and_inflections() {
        for w in ["time", "running", "worked", "worm", "silent", "hunter", "the", "a"] {
            assert!(is_word(w), "{w} should be a word");
        }
        assert!(is_word("Time"));
        assert!(is_word("RUNNING"));
    }

    #[test]
    fn non_words_rejected() {
        for w in ["xqzt", "", "zzzxq", "kikimora"] {
            assert!(!is_word(w), "{w} should NOT be a dictionary word");
        }
    }
}
