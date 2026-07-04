fn is_distance(s: &str) -> bool {
    let s = s.trim();
    s == "-"
        || s == "*"
        || s.ends_with("km")
        || s.ends_with("AU")
        || s.ends_with(" m")
}

pub fn looks_like_dscan(text: &str) -> Option<usize> {
    let lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
    if lines.is_empty() {
        return None;
    }
    let ok = lines
        .iter()
        .filter(|l| {
            let cols: Vec<&str> = l.split('\t').collect();
            cols.len() >= 3 && is_distance(cols.last().unwrap())
        })
        .count();
    (ok == lines.len()).then_some(ok)
}

/// EVE character-name rules: 3–37 characters; ASCII letters/digits plus apostrophe and
/// hyphen, in 1–3 space-separated words (no empty/leading/trailing/double spaces); the family
/// (last) word ≤ 12 and the given part (the rest) ≤ 24; at least one letter.
pub fn is_valid_char_name(s: &str) -> bool {
    let s = s.trim();
    let len = s.chars().count();
    if !(3..=37).contains(&len) {
        return false;
    }
    if !s.chars().all(|c| c.is_ascii_alphanumeric() || c == ' ' || c == '\'' || c == '-') {
        return false;
    }
    if !s.chars().any(|c| c.is_ascii_alphabetic()) {
        return false;
    }
    let words: Vec<&str> = s.split(' ').collect();
    if words.is_empty() || words.len() > 3 || words.iter().any(|w| w.is_empty()) {
        return false;
    }
    if words[words.len() - 1].chars().count() > 12 {
        return false;
    }
    if words.len() > 1 && words[..words.len() - 1].join(" ").chars().count() > 24 {
        return false;
    }
    true
}

pub fn looks_like_local(text: &str) -> Option<usize> {
    let names: Vec<&str> = text
        .lines()
        .map(|l| l.split('\t').next().unwrap_or(l).trim())
        .filter(|l| !l.is_empty())
        .collect();
    if names.len() < 3 {
        return None;
    }
    names.iter().all(|l| is_valid_char_name(l)).then_some(names.len())
}

pub fn upload(text: &str) -> anyhow::Result<String> {
    let client = reqwest::blocking::Client::builder()
        .user_agent("eve-spai")
        .timeout(std::time::Duration::from_secs(20))
        .build()?;
    // The site's form POSTs `paste=<text>` to "/" and replies "OK;<id>" / "ERROR;<msg>".
    let body = client
        .post("https://dscan.info/")
        .form(&[("paste", text)])
        .send()?
        .error_for_status()?
        .text()?;
    let mut parts = body.splitn(2, ';');
    match (parts.next(), parts.next()) {
        (Some("OK"), Some(id)) => Ok(format!("https://dscan.info/v/{}", id.trim())),
        (Some("ERROR"), Some(msg)) => anyhow::bail!("dscan.info: {}", msg.trim()),
        _ => anyhow::bail!("unexpected dscan.info response"),
    }
}

/// Parse the ship breakdown out of a dscan.info share PAGE. dscan.info serves a share as HTML (there
/// is no data/JSON API), rendering the ships as a `<ul id="ships">` list where each entry is
/// `<li ...><span class="badge...">COUNT</span><b>TYPE NAME</b></li>`. Returns `(type name, count)`
/// pairs in document order; the caller maps names to type ids via the ship index. Empty if the page
/// has no ships block (e.g. a bad id, or a local/fleet scan rather than a d-scan).
pub fn parse_dscan_ships_html(body: &str) -> Vec<(String, u32)> {
    let Some(start) = body.find("id=\"ships\"") else {
        return Vec::new();
    };
    let rest = &body[start..];
    let block = &rest[..rest.find("</ul>").unwrap_or(rest.len())];
    let mut out = Vec::new();
    for li in block.split("<li").skip(1) {
        let count = li
            .find("badge")
            .and_then(|b| li[b..].find('>').map(|g| b + g + 1))
            .map(|s| li[s..].split('<').next().unwrap_or("").trim())
            .and_then(|s| s.parse::<u32>().ok())
            .unwrap_or(1);
        if let Some(b) = li.find("<b>") {
            if let Some(e) = li[b + 3..].find("</b>") {
                let name = li[b + 3..b + 3 + e].trim();
                if !name.is_empty() {
                    out.push((name.to_owned(), count));
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_ships_from_real_dscan_info_page() {
        let html = r#"<ul class="list-group" id="ships">
            <li class="list-group-item shipclass27" data-sclid="27">
              <span class="badge label label-default">14</span>
              <b>Raven</b>
            </li>
            <li class="list-group-item shipclass358" data-sclid="358">
              <span class="badge label label-default">2</span>
              <b>Ishtar</b>
            </li>
            <li class="list-group-item shipclass831" data-sclid="831">
              <span class="badge label label-default">1</span>
              <b>Crow</b>
            </li>
          </ul>
          <ul class="list-group" id="shipclasses"><li><b>Battleship</b></li></ul>"#;
        let ships = parse_dscan_ships_html(html);
        assert_eq!(
            ships,
            vec![("Raven".to_owned(), 14), ("Ishtar".to_owned(), 2), ("Crow".to_owned(), 1)]
        );
        assert!(parse_dscan_ships_html("<html><body>nope</body></html>").is_empty());
    }

    #[test]
    fn detects_dscan_and_rejects_other() {
        let scan = "12345\tSome Rifter\tRifter\t1,234 km\n\
                    67890\tGate\tStargate\t-\n\
                    11111\tProbe\tCore Scanner Probe\t12.3 AU";
        assert_eq!(looks_like_dscan(scan), Some(3));
        assert_eq!(looks_like_dscan(&format!("{scan}\n")), Some(3));
        assert_eq!(looks_like_dscan("hello world"), None);
        assert_eq!(looks_like_dscan("a\tb"), None);
        assert_eq!(looks_like_dscan("name\ttype\tno-distance-here"), None);
    }

    #[test]
    fn valid_char_names() {
        assert!(is_valid_char_name("Death Eater 101"));
        assert!(is_valid_char_name("ji wuming"));
        assert!(is_valid_char_name("O'Neil"));
        assert!(is_valid_char_name("Al-Khwarizmi Bin Musa"));
        assert!(!is_valid_char_name("ab"));
        assert!(!is_valid_char_name("a\tb"));
        assert!(!is_valid_char_name("too  many   spaces"));
        assert!(!is_valid_char_name("one two three four"));
        assert!(!is_valid_char_name("Has=Bad/Chars"));
        assert!(!is_valid_char_name("ThisFamilyNameWayTooLong"));
    }

    #[test]
    fn detects_local_member_list() {
        let local = "Death Eater 101\nji wuming\nO'Neil\nMittani\nThe Mittani";
        assert_eq!(looks_like_local(local), Some(5));
        assert_eq!(looks_like_local("Alpha One\nBravo Two"), None);
        assert_eq!(looks_like_local("Alpha One\nBravo Two\nnot a name!!!"), None);
        assert_eq!(looks_like_local("12345\tRifter\tRifter\t1 km\nx\ty\tz\t2 km\na\tb\tc\t3 km"), None);
    }
}
