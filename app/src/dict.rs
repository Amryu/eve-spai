//! Bundled English dictionary, used to auto-filter single-word lowercase prose words that would
//! otherwise be queued to ESI as pilot candidates (see `intel::is_lowercaseish` and the single-word
//! drop in `analyze_ctx`). The wordlist is dwyl/english-words `words_alpha.txt` (~370k words, public
//! domain / Unlicense), sorted (LC_ALL=C, i.e. byte order — matches `str` Ord for ASCII) and
//! gzip-compressed at build time. It is decompressed once, lazily, into a sorted slice that is
//! searched by binary search.

use std::sync::LazyLock;

/// Gzip-compressed, byte-sorted, newline-separated lowercase word list.
static WORDS_GZ: &[u8] = include_bytes!("../assets/english_words.txt.gz");

/// The decompressed word list as a sorted slice of lowercase words (binary-searchable).
static DICT: LazyLock<Box<[Box<str>]>> = LazyLock::new(load);

fn load() -> Box<[Box<str>]> {
    use std::io::Read;
    let mut text = String::new();
    if flate2::read::GzDecoder::new(WORDS_GZ).read_to_string(&mut text).is_err() {
        return Box::new([]);
    }
    text.lines().map(Box::<str>::from).collect()
}

/// Whether `w` is a known basic English word (case-insensitive). The empty string is not a word.
pub fn is_word(w: &str) -> bool {
    if w.is_empty() {
        return false;
    }
    let lw = w.to_ascii_lowercase();
    DICT.binary_search_by(|entry| entry.as_ref().cmp(lw.as_str())).is_ok()
}

/// Force the lazy decompression now (call once at startup, off the UI thread) so the first parse
/// doesn't pay the ~370k-word decompress on the hot path.
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
        assert!(is_word("Time")); // case-insensitive
        assert!(is_word("RUNNING"));
    }

    #[test]
    fn non_words_rejected() {
        for w in ["xqzt", "", "zzzxq", "kikimora"] {
            assert!(!is_word(w), "{w} should NOT be a dictionary word");
        }
    }
}
