//! Cuts the model's streaming text into sentences to speak, so speech starts while the answer is
//! still being written, and strips what reads badly aloud: markdown, link targets, web addresses.

/// A sentence shorter than this waits for the next, so speech does not stutter on "Yes." "No."
const MIN_CHARS: usize = 25;

#[derive(Default)]
pub struct Chunker {
    buf: String,
}

impl Chunker {
    /// Takes more text; returns the sentences now complete.
    pub fn push(&mut self, text: &str) -> Vec<String> {
        self.buf.push_str(text);
        let mut out = Vec::new();
        while let Some(end) = sentence_end(&self.buf, MIN_CHARS) {
            let s: String = self.buf.drain(..end).collect();
            let s = speakable(&s);
            if !s.is_empty() {
                out.push(s);
            }
        }
        out
    }

    /// The rest, at the end of the answer.
    pub fn finish(&mut self) -> Option<String> {
        let s = speakable(&std::mem::take(&mut self.buf));
        (!s.is_empty()).then_some(s)
    }
}

/// Byte offset just past the first sentence end in `s` that leaves at least `min` characters, or
/// None while the sentence is still open.
fn sentence_end(s: &str, min: usize) -> Option<usize> {
    let mut count = 0;
    let mut it = s.char_indices().peekable();
    while let Some((i, c)) = it.next() {
        count += 1;
        let next = it.peek().map(|&(_, n)| n);
        let end = match c {
            '\n' => true,
            // CJK full stops need no space after them.
            '。' | '！' | '？' => true,
            '.' | '!' | '?' | ';' => match next {
                Some(n) => n.is_whitespace() && !abbreviation(&s[..i]),
                None => false,
            },
            _ => false,
        };
        if end && count >= min {
            return Some(i + c.len_utf8());
        }
    }
    None
}

/// "e.g." and "approx." are not the end of a sentence.
fn abbreviation(before: &str) -> bool {
    let word: String = before.chars().rev().take_while(|c| c.is_alphabetic() || *c == '.').collect::<String>().chars().rev().collect();
    matches!(word.to_lowercase().as_str(), "e.g" | "i.e" | "etc" | "approx" | "vs" | "z.b" | "bzw" | "ca" | "dr" | "mr" | "mrs")
}

/// What is said for a piece of the model's text.
pub fn speakable(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    // Links read as their label.
    while let Some(open) = rest.find('[') {
        match (rest[open..].find("]("), rest[open..].find(')')) {
            (Some(mid), Some(close)) if mid < close => {
                out.push_str(&rest[..open]);
                out.push_str(&rest[open + 1..open + mid]);
                rest = &rest[open + close + 1..];
            }
            _ => {
                out.push_str(&rest[..=open]);
                rest = &rest[open + 1..];
            }
        }
    }
    out.push_str(rest);
    let words: Vec<&str> = out.split_whitespace().filter(|w| !w.starts_with("http://") && !w.starts_with("https://")).collect();
    let mut t = words.join(" ");
    for m in ["**", "__", "`", "###", "##", "# "] {
        t = t.replace(m, "");
    }
    let t = t.trim_start_matches(['-', '*', '•', ' ']).replace(" - ", ", ");
    t.trim().to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sentences_come_out_as_the_text_streams_in() {
        let mut c = Chunker::default();
        assert!(c.push("The Frat gang went sou").is_empty());
        assert_eq!(c.push("th. Last seen in QX-LIJ 4 minutes ago, 6 jumps out. Then").as_slice(), ["The Frat gang went south.", "Last seen in QX-LIJ 4 minutes ago, 6 jumps out."]);
        assert_eq!(c.finish().as_deref(), Some("Then"));
    }

    #[test]
    fn short_sentences_numbers_and_abbreviations_hold_together() {
        let mut c = Chunker::default();
        assert!(c.push("Yes. It is 2.5 ly away, e.g. from Jita").is_empty(), "too short, a decimal, an abbreviation");
        assert_eq!(c.push(". Next").len(), 1);
    }

    #[test]
    fn markdown_links_and_addresses_are_not_read_out() {
        assert_eq!(speakable("- **Killed** [a Hound](spai:kill/1) in `1DQ1-A`, see https://zkillboard.com/kill/1/"), "Killed a Hound in 1DQ1-A, see");
        assert_eq!(speakable("## Summary"), "Summary");
    }

    #[test]
    fn chinese_and_line_breaks_end_sentences() {
        let mut c = Chunker::default();
        let out = c.push("法兄弟会舰队向南移动，最后出现在QX-LIJ，四分钟前。下一句");
        assert_eq!(out.len(), 1);
        let mut c = Chunker::default();
        assert_eq!(c.push("A list of the systems they passed\nnext").len(), 1);
    }
}
