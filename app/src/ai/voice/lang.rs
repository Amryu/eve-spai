//! Which language a sentence is in, for picking a voice that can say it. Only the languages the
//! assistant answers in: English, German, Spanish, Russian and Chinese.

pub fn detect(text: &str) -> &'static str {
    let (mut cyr, mut han, mut letters) = (0, 0, 0);
    for c in text.chars() {
        if ('\u{0400}'..='\u{04FF}').contains(&c) {
            cyr += 1;
        } else if ('\u{4E00}'..='\u{9FFF}').contains(&c) || ('\u{3400}'..='\u{4DBF}').contains(&c) {
            han += 1;
        } else if c.is_alphabetic() {
            letters += 1;
        }
    }
    if han > 0 && han * 2 >= letters / 3 {
        return "zh";
    }
    if cyr > letters {
        return "ru";
    }
    let lower = text.to_lowercase();
    let words: Vec<&str> = lower.split(|c: char| !c.is_alphabetic()).filter(|w| !w.is_empty()).collect();
    let score = |list: &[&str]| words.iter().filter(|w| list.contains(w)).count() + lower.chars().filter(|c| !c.is_ascii() && list.iter().any(|l| l.chars().count() == 1 && l.starts_with(*c))).count();
    let de = score(&["der", "die", "das", "und", "ist", "nicht", "ein", "eine", "mit", "von", "sind", "auf", "zu", "im", "sprünge", "ß", "ä", "ö", "ü"]);
    let es = score(&["el", "la", "los", "las", "y", "es", "que", "un", "una", "con", "por", "saltos", "está", "ñ", "¿", "¡", "á", "é", "í", "ó", "ú"]);
    let en = score(&["the", "and", "is", "of", "to", "in", "a", "with", "jumps", "they", "it", "from"]);
    if de > en && de >= es {
        "de"
    } else if es > en && es > de {
        "es"
    } else {
        "en"
    }
}

#[cfg(test)]
mod tests {
    use super::detect;

    #[test]
    fn the_five_languages_are_told_apart() {
        assert_eq!(detect("The Frat gang went south, 6 jumps from you."), "en");
        assert_eq!(detect("Die Frat-Gang ist nach Süden geflogen, 6 Sprünge von dir."), "de");
        assert_eq!(detect("La flota de Frat está a 6 saltos, en QX-LIJ."), "es");
        assert_eq!(detect("Флот Frat ушёл на юг, в QX-LIJ, 6 прыжков."), "ru");
        assert_eq!(detect("Frat舰队向南移动，最后出现在QX-LIJ。"), "zh");
        assert_eq!(detect("1DQ1-A"), "en");
    }
}
