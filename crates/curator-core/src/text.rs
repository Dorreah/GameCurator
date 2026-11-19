//! Small text helpers: search normalization, title cleanup, HTML escaping.

// Latin letters with diacritics and their base letters (Latin-1 Supplement,
// Latin Extended-A and the Romanian/Turkish extras). A compact table instead
// of full Unicode normalization tables keeps the executable small; game
// titles in other scripts are still matched case-insensitively.
const FOLD_FROM: &str = "ÀÁÂÃÄÅàáâãäåĀāĂăĄąÇçĆćĈĉĊċČčĎďĐđÈÉÊËèéêëĒēĔĕĖėĘęĚěĜĝĞğĠġĢģĤĥĦħÌÍÎÏìíîïĨĩĪīĬĭĮįİıĴĵĶķĹĺĻļĽľĿŀŁłÑñŃńŅņŇňÒÓÔÕÖØòóôõöøŌōŎŏŐőŔŕŖŗŘřŚśŜŝŞşŠšŢţŤťŦŧÙÚÛÜùúûüŨũŪūŬŭŮůŰűŲųŴŵÝýÿŶŷŸŹźŻżŽžȘșȚțÐðÞþ";
const FOLD_TO: &str = "AAAAAAaaaaaaAaAaAaCcCcCcCcCcDdDdEEEEeeeeEeEeEeEeEeGgGgGgGgHhHhIIIIiiiiIiIiIiIiIiJjKkLlLlLlLlLlNnNnNnNnOOOOOOooooooOoOoOoRrRrRrSsSsSsSsTtTtTtUUUUuuuuUuUuUuUuUuUuWwYyyYyYZzZzZzSsTtDdTt";

fn fold_char(c: char, out: &mut String) {
    if c.is_ascii() {
        out.push(c.to_ascii_lowercase());
        return;
    }
    // combining diacritical marks (decomposed input) are dropped
    if ('\u{0300}'..='\u{036F}').contains(&c) {
        return;
    }
    match c {
        'ß' => out.push_str("ss"),
        'Æ' | 'æ' => out.push_str("ae"),
        'Œ' | 'œ' => out.push_str("oe"),
        _ => match FOLD_FROM.chars().position(|f| f == c) {
            Some(i) => out.push(FOLD_TO.chars().nth(i).unwrap_or(c).to_ascii_lowercase()),
            None => out.extend(c.to_lowercase()),
        },
    }
}

/// Lowercase and strip diacritics, so that "Zaklínač" matches "zaklinac".
pub fn search_key(s: &str) -> String {
    let mut folded = String::with_capacity(s.len());
    for c in s.chars() {
        fold_char(c, &mut folded);
    }
    folded.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Escapes `%`, `_` and `\` for use inside `LIKE ... ESCAPE '\'`.
pub fn escape_like(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if matches!(c, '%' | '_' | '\\') {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

pub fn escape_html(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            c => out.push(c),
        }
    }
    out
}

/// Collapses whitespace and trims.
pub fn tidy(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn looks_like_version(token: &str) -> bool {
    let t = token.trim_matches(|c: char| c == '(' || c == ')' || c == '[' || c == ']');
    if t.is_empty() {
        return false;
    }
    let lower = t.to_lowercase();
    let body = lower.strip_prefix('v').unwrap_or(&lower);
    let digits = body.chars().filter(|c| c.is_ascii_digit()).count();
    // "1.0.2", "v2.5", "20231105", "1.0.2a"
    digits > 0
        && body.chars().all(|c| c.is_ascii_digit() || c == '.' || c == '-' || c.is_ascii_lowercase())
        && (body.contains('.') || (digits >= 5 && digits == body.len()) || lower.starts_with('v') && digits == body.len())
}

const NOISE_WORDS: &[&str] = &["gog", "drm", "free", "drmfree", "drm-free", "setup", "installer", "offline", "win", "windows", "x64", "x86", "64bit", "32bit", "en", "multi"];

/// Turns a folder or installer name into a readable title.
/// `Baldurs_Gate_3_v4.1.1_[GOG]` -> `Baldurs Gate 3`.
pub fn clean_title(raw: &str) -> String {
    // Remove bracketed segments: [GOG], (v1.2), (12345)
    let mut s = String::new();
    let mut depth = 0i32;
    for c in raw.chars() {
        match c {
            '[' | '(' | '{' => depth += 1,
            ']' | ')' | '}' => depth = (depth - 1).max(0),
            _ if depth == 0 => s.push(c),
            _ => {}
        }
    }
    let mut s = s.replace('_', " ");
    if !s.contains(' ') && s.matches('.').count() >= 1 {
        s = s.replace('.', " ");
    }
    let tokens: Vec<&str> = s.split_whitespace().collect();
    let mut kept: Vec<String> = Vec::new();
    for tok in tokens {
        let lower = tok.to_lowercase();
        if NOISE_WORDS.contains(&lower.as_str()) || looks_like_version(tok) {
            continue;
        }
        kept.push(tok.trim_matches(|c: char| c == '-' || c == '.').to_string());
    }
    kept.retain(|t| !t.is_empty());
    let joined = kept.join(" ");
    let result = if joined.is_empty() { tidy(raw) } else { joined };
    // all-lowercase names (typical for installer files) get title case
    if result.chars().any(|c| c.is_alphabetic()) && !result.chars().any(|c| c.is_uppercase()) {
        result
            .split(' ')
            .map(|w| {
                let mut cs = w.chars();
                match cs.next() {
                    Some(f) => f.to_uppercase().collect::<String>() + cs.as_str(),
                    None => String::new(),
                }
            })
            .collect::<Vec<_>>()
            .join(" ")
    } else {
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_key_strips_diacritics() {
        assert_eq!(FOLD_FROM.chars().count(), FOLD_TO.chars().count(), "fold table out of sync");
        assert_eq!(search_key("Zaklínač  3: Divoký hon"), "zaklinac 3: divoky hon");
        assert_eq!(search_key("ŽLUŤOUČKÝ kůň úpěl ďábelské ódy"), "zlutoucky kun upel dabelske ody");
        assert_eq!(search_key("Straße Œuvre Ærø Łódź Ñandú Şişli Mărțișor"), "strasse oeuvre aero lodz nandu sisli martisor");
        assert_eq!(search_key("Cafe\u{301}"), "cafe", "decomposed input");
        assert_eq!(search_key("ВЕДЬМАК 3"), "ведьмак 3", "other scripts: case-insensitive");
    }

    #[test]
    fn like_escaping() {
        assert_eq!(escape_like("100%_a\\"), "100\\%\\_a\\\\");
    }

    #[test]
    fn titles_are_cleaned() {
        assert_eq!(clean_title("Baldurs_Gate_3_v4.1.1_[GOG]"), "Baldurs Gate 3");
        assert_eq!(clean_title("System.Shock.2"), "System Shock 2");
        assert_eq!(clean_title("Disco Elysium (GOG)"), "Disco Elysium");
        assert_eq!(clean_title("setup_disco_elysium_1.0.2"), "Disco Elysium");
        assert_eq!(clean_title("Fallout 2"), "Fallout 2");
        assert_eq!(clean_title("1602 A.D."), "1602 A.D");
        assert_eq!(clean_title("setup_witcher_3_wild_hunt_4.04_(gog-3)_(53456)"), "Witcher 3 Wild Hunt");
    }
}
