//! Bundled English word list (dwyl/english-words, public domain), used to drop single-word
//! lowercase prose before it is queued to ESI as a pilot. Byte-sorted so `str` Ord can
//! binary-search it.

use std::sync::LazyLock;

/// Gzip-compressed, byte-sorted, newline-separated lowercase word list.
static WORDS_GZ: &[u8] = include_bytes!("../assets/english_words.txt.gz");

static DICT: LazyLock<Dict> = LazyLock::new(load);

/// The words back to back in one buffer, with where each starts: a boxed string per word cost four
/// times the text, about 17 MB for 370k words.
struct Dict {
    text: String,
    starts: Vec<u32>,
}

impl Dict {
    fn word(&self, i: usize) -> &str {
        let end = self.starts.get(i + 1).map_or(self.text.len(), |&e| e as usize);
        &self.text[self.starts[i] as usize..end]
    }

    fn contains(&self, w: &str) -> bool {
        let (mut lo, mut hi) = (0, self.starts.len());
        while lo < hi {
            let mid = (lo + hi) / 2;
            match self.word(mid).cmp(w) {
                std::cmp::Ordering::Less => lo = mid + 1,
                std::cmp::Ordering::Greater => hi = mid,
                std::cmp::Ordering::Equal => return true,
            }
        }
        false
    }
}

fn load() -> Dict {
    use std::io::Read;
    let mut raw = String::new();
    if flate2::read::GzDecoder::new(WORDS_GZ).read_to_string(&mut raw).is_err() {
        return Dict { text: String::new(), starts: Vec::new() };
    }
    let mut text = String::with_capacity(raw.len());
    let mut starts = Vec::with_capacity(raw.len() / 9);
    for line in raw.lines() {
        starts.push(text.len() as u32);
        text.push_str(line);
    }
    text.shrink_to_fit();
    starts.shrink_to_fit();
    Dict { text, starts }
}

pub fn is_word(w: &str) -> bool {
    if w.is_empty() {
        return false;
    }
    DICT.contains(&w.to_ascii_lowercase())
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
    fn every_listed_word_is_found_in_a_compact_buffer() {
        let d = &*DICT;
        assert!(d.starts.len() > 300_000);
        for i in 0..d.starts.len() {
            assert!(i == 0 || d.word(i - 1) <= d.word(i), "the list must stay byte-sorted for the search");
            assert!(d.contains(d.word(i)), "{} not found", d.word(i));
        }
        let bytes = d.text.capacity() + d.starts.capacity() * 4;
        assert!(bytes < 7_000_000, "{bytes} bytes");
    }

    #[test]
    fn non_words_rejected() {
        for w in ["xqzt", "", "zzzxq", "kikimora"] {
            assert!(!is_word(w), "{w} should NOT be a dictionary word");
        }
    }
}
