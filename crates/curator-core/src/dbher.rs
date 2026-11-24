//! Czech game details from Databáze-her.cz.
//!
//! Databáze-her.cz is the big Czech game database; its game profiles are
//! published under CC BY-NC-ND 3.0 CZ, which allows personal, non-commercial
//! use with the author and a link to the page (`lookup::Details::attribution`
//! writes it under the description) and without changes to the text.
//!
//! Only the public game pages are read (`/hry/<name>/`): the address is made
//! from the game's title the way the site makes it ("Baldur's Gate III" ->
//! `baldurs-gate-iii`), with a few variants (Roman numerals, without the
//! edition or the subtitle). The site's search pages are left alone - its
//! robots.txt asks programs not to use them. When the address cannot be
//! guessed, the user pastes the page address (or opens the site's search in
//! the browser and copies it from there).
//!
//! A page gives the title (and other names), a Czech description, genres,
//! developers and release dates. Publishers and covers are not taken.

use crate::lookup::{core_title, html_to_text, rank_any, Candidate, Fetcher, LookupResult, NetError, Source};
use crate::text;

/// Where the site is (tests use a local server).
#[derive(Debug, Clone)]
pub struct Endpoints {
    pub base: String,
    /// Tests only: plain HTTP to 127.0.0.1 / localhost is allowed too.
    pub allow_local_http: bool,
}

impl Endpoints {
    pub fn site() -> Self {
        Endpoints { base: "https://www.databaze-her.cz".into(), allow_local_http: false }
    }
}

/// The public address of a game page.
pub const PAGE_BASE: &str = "https://www.databaze-her.cz/hry/";

// ---------------------------------------------------------------- page addresses

/// The page name the site makes from a title: lower case without
/// diacritics, "&" = "and", apostrophes and dots left out, other characters
/// become dashes ("S.T.A.L.K.E.R.: Shadow of Chernobyl" ->
/// `stalker-shadow-of-chernobyl`).
pub fn slug(title: &str) -> String {
    let s = text::search_key(&title.replace('&', " and "));
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if matches!(c, '\'' | '\u{2019}' | '\u{2018}' | '`' | '.' | '\u{2122}' | '\u{ae}' | '\u{a9}') {
            // "Baldur's" = "baldurs", "S.T.A.L.K.E.R." = "stalker"
        } else if !out.ends_with('-') {
            out.push('-');
        }
    }
    out.trim_matches('-').to_string()
}

fn roman(n: u32) -> Option<&'static str> {
    const R: [&str; 19] = ["ii", "iii", "iv", "v", "vi", "vii", "viii", "ix", "x", "xi", "xii", "xiii", "xiv", "xv", "xvi", "xvii", "xviii", "xix", "xx"];
    (2..=20).contains(&n).then(|| R[(n - 2) as usize])
}

/// The same page name with numbers 2-20 as Roman numerals ("baldurs-gate-3"
/// -> "baldurs-gate-iii").
pub fn roman_slug(slug: &str) -> String {
    slug.split('-').map(|w| w.parse::<u32>().ok().and_then(roman).unwrap_or(w)).collect::<Vec<_>>().join("-")
}

/// Page names to try for a title, most likely first (at most five).
pub fn slug_variants(title: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut push = |s: String| {
        if !s.is_empty() && !out.contains(&s) && out.len() < 5 {
            out.push(s);
        }
    };
    let full = slug(title);
    push(full.clone());
    push(roman_slug(&full));
    // without the edition words ("... Enhanced Edition", "GOTY")
    let core = slug(&core_title(title));
    let starts_with_the = full.starts_with("the-");
    push(if starts_with_the { format!("the-{core}") } else { core.clone() });
    push(roman_slug(&core));
    // without the subtitle ("Fallout: A Post Nuclear ...")
    if let Some(head) = title.split([':', '\u{2013}', '\u{2014}']).next().filter(|h| h.len() < title.len()) {
        push(slug(head));
    }
    out
}

/// The page name in a page address the user pasted
/// (`https://www.databaze-her.cz/hry/system-shock-2/`), if it is one.
pub fn slug_from_input(input: &str) -> Option<String> {
    let s = input.trim();
    let lower = s.to_ascii_lowercase();
    if !lower.contains("databaze-her.cz") {
        return None;
    }
    let after = &s[lower.find("/hry/")? + 5..];
    let name: String = after.split(['/', '?', '#']).next()?.to_ascii_lowercase();
    valid_slug(&name).then_some(name)
}

fn valid_slug(s: &str) -> bool {
    !s.is_empty() && s.len() <= 150 && s.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

// ---------------------------------------------------------------- reading a page

/// The inner HTML of the first element whose opening tag contains `marker`
/// (e.g. `class="game-description`), up to its matching closing tag.
fn element<'a>(html: &'a str, tag: &str, marker: &str) -> Option<&'a str> {
    let at = html.find(marker)?;
    let open_start = html[..at + marker.len()].rfind(&format!("<{tag}"))?;
    let content_start = open_start + html[open_start..].find('>')? + 1;
    let (open, close) = (format!("<{tag}"), format!("</{tag}>"));
    let mut depth = 1usize;
    let mut i = content_start;
    while depth > 0 {
        let next_open = html[i..].find(&open).map(|p| p + i);
        let next_close = html[i..].find(&close).map(|p| p + i)?;
        match next_open {
            Some(o) if o < next_close => {
                depth += 1;
                i = o + open.len();
            }
            _ => {
                depth -= 1;
                if depth == 0 {
                    return Some(&html[content_start..next_close]);
                }
                i = next_close + close.len();
            }
        }
    }
    None
}

/// The texts of the links in a piece of HTML.
fn link_texts(html: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = html;
    while let Some(a) = rest.find("<a ") {
        let Some(gt) = rest[a..].find('>') else { break };
        let start = a + gt + 1;
        let Some(end) = rest[start..].find("</a>") else { break };
        let t = text::tidy(&html_to_text(&rest[start..start + end]));
        if !t.is_empty() {
            out.push(t);
        }
        rest = &rest[start + end + 4..];
    }
    out
}

/// The VideoGame data of the page (schema.org JSON-LD), if it has one.
fn json_ld(html: &str) -> Option<serde_json::Value> {
    let mut rest = html;
    while let Some(i) = rest.find("application/ld+json") {
        let start = i + rest[i..].find('>')? + 1;
        let end = start + rest[start..].find("</script>")?;
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&rest[start..end]) {
            if v.get("@type").and_then(|t| t.as_str()) == Some("VideoGame") {
                return Some(v);
            }
        }
        rest = &rest[end..];
    }
    None
}

fn first_upper(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}

/// Years (1940-2100) written in a text ("19.05.2015" -> 2015).
fn years_in(text: &str) -> Vec<i64> {
    let b = text.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i + 4 <= b.len() {
        let four = b[i..i + 4].iter().all(u8::is_ascii_digit);
        let before = i == 0 || !b[i - 1].is_ascii_digit();
        let after = i + 4 == b.len() || !b[i + 4].is_ascii_digit();
        if four && before && after {
            if let Ok(y) = text[i..i + 4].parse::<i64>() {
                if (1940..=2100).contains(&y) {
                    out.push(y);
                }
            }
            i += 4;
        } else {
            i += 1;
        }
    }
    out
}

/// The release year: the PC release if the page lists one, otherwise the
/// first release.
fn release_year(dates_html: &str) -> Option<i64> {
    let mut pc: Option<i64> = None;
    let mut first: Option<i64> = None;
    for part in dates_html.split("class=\"game-date\"").skip(1) {
        let years = years_in(&html_to_text(&format!("<x {part}")));
        let Some(y) = years.into_iter().min() else { continue };
        if part.contains("data-system-name=\"pc\"") {
            pc = Some(pc.map_or(y, |p: i64| p.min(y)));
        }
        first = Some(first.map_or(y, |f: i64| f.min(y)));
    }
    pc.or(first)
}

/// Reads a game page. `None` when it is not a game page.
pub fn parse_page(html: &str, slug: &str) -> Option<Candidate> {
    let ld = json_ld(html);
    let ld_str = |k: &str| ld.as_ref().and_then(|v| v.get(k)).and_then(|x| x.as_str()).map(text::tidy).filter(|s| !s.is_empty());
    let head = element(html, "div", "class=\"game-title\"");
    let title = head
        .and_then(|h| element(h, "h1", "<h1"))
        .map(|t| text::tidy(&html_to_text(t)))
        .filter(|t| !t.is_empty())
        .or_else(|| ld_str("name"))?;
    let mut c = Candidate::new(slug, crate::lookup::cut(&title, 200));
    c.store_link = format!("{PAGE_BASE}{slug}/");

    // other names (translations, re-releases), comma separated under the title
    if let Some(h4) = head.and_then(|h| element(h, "h4", "<h4")) {
        c.alt_titles = html_to_text(h4).split(", ").map(text::tidy).filter(|s| !s.is_empty() && s.chars().count() <= 200).take(12).collect();
    }

    // developers: the page data first, the header links otherwise
    let names = |v: &serde_json::Value| -> Vec<String> {
        match v {
            serde_json::Value::Array(a) => a
                .iter()
                .filter_map(|x| x.as_str().map(str::to_string).or_else(|| x.get("name").and_then(|n| n.as_str()).map(str::to_string)))
                .map(|s| text::tidy(&s))
                .filter(|s| !s.is_empty())
                .collect(),
            serde_json::Value::String(s) => vec![text::tidy(s)],
            serde_json::Value::Object(_) => v.get("name").and_then(|n| n.as_str()).map(|n| vec![text::tidy(n)]).unwrap_or_default(),
            _ => Vec::new(),
        }
    };
    c.developers = ld.as_ref().and_then(|v| v.get("author")).map(names).unwrap_or_default();
    if c.developers.is_empty() {
        if let Some(d) = element(html, "div", "class=\"game-developers\"") {
            c.developers = link_texts(d);
        }
    }

    // genres ("akce", "RPG") and their kinds ("1st person akce") as tags
    c.genres = ld.as_ref().and_then(|v| v.get("genre")).map(names).unwrap_or_default().iter().map(|g| first_upper(g)).collect();
    if let Some(g) = element(html, "dl", "class=\"game-genres") {
        let mut main = Vec::new();
        for line in g.split("<div").skip(1) {
            let links = link_texts(line);
            if let Some(top) = links.first() {
                let top = first_upper(top);
                if !main.contains(&top) {
                    main.push(top);
                }
            }
            if let Some(kind) = links.get(1) {
                let lower = kind.to_lowercase();
                if lower != "nezařazeno" && lower != "ostatní" && !c.tags.iter().any(|t: &String| t.to_lowercase() == lower) {
                    c.tags.push(kind.clone());
                }
            }
        }
        if c.genres.is_empty() {
            c.genres = main;
        }
    }

    c.year = element(html, "div", "class=\"game-dates\"").and_then(release_year);

    // an add-on ("datadisk", "DLC") rather than a game
    if let Some(i) = html.find("<dt>Rozsah</dt>") {
        let end = html[i..].find("</dd>").map_or(html.len(), |e| i + e);
        let dd = html[i..end].to_lowercase();
        if dd.contains("datadisk") || dd.contains("dlc") || dd.contains("rozšíření") {
            c.product_type = "dlc".into();
        }
    }

    c.description = element(html, "div", "class=\"game-description").map(|d| html_to_text(&d.replace('\r', ""))).filter(|d| !d.is_empty());
    Some(c)
}

// ---------------------------------------------------------------- the client

pub struct Client {
    f: Fetcher,
    ep: Endpoints,
}

impl Client {
    pub fn new(ep: Endpoints) -> Self {
        Client { f: Fetcher::new(Source::Dbher, ep.allow_local_http), ep }
    }

    /// One game page; `None` when the site has no page of that name.
    pub fn page(&self, slug: &str) -> LookupResult<Option<Candidate>> {
        if !valid_slug(slug) {
            return Err(self.f.err(NetError::Bad("page name".into())));
        }
        let url = format!("{}/hry/{slug}/", self.ep.base.trim_end_matches('/'));
        match self.f.get_text(&url) {
            Ok(html) => Ok(parse_page(&html, slug)),
            Err(e) if e.kind == NetError::NotFound => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// Looks a title up by trying the page names it would have; the first
    /// page found is the candidate (scored against the title and the page's
    /// other names).
    pub fn search(&self, title: &str) -> LookupResult<Vec<Candidate>> {
        for s in slug_variants(title) {
            if let Some(c) = self.page(&s)? {
                return Ok(rank_any(&[title], vec![c]));
            }
        }
        Ok(Vec::new())
    }

    /// What the user typed: a page address is read as it is, anything else
    /// is a title to look for.
    pub fn lookup(&self, input: &str, title: &str) -> LookupResult<Vec<Candidate>> {
        match slug_from_input(input) {
            Some(s) => Ok(self.page(&s)?.map(|c| rank_any(&[title, &c.title.clone()], vec![c])).unwrap_or_default()),
            None => self.search(input).map(|found| rank_any(&[title, input], found)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_names_are_made_like_the_site_makes_them() {
        assert_eq!(slug("System Shock 2"), "system-shock-2");
        assert_eq!(slug("Baldur's Gate III"), "baldurs-gate-iii");
        assert_eq!(slug("The Witcher 3: Wild Hunt"), "the-witcher-3-wild-hunt");
        assert_eq!(slug("Command & Conquer: Red Alert 2"), "command-and-conquer-red-alert-2");
        assert_eq!(slug("S.T.A.L.K.E.R.: Shadow of Chernobyl"), "stalker-shadow-of-chernobyl");
        assert_eq!(slug("Mafia: The City of Lost Heaven™"), "mafia-the-city-of-lost-heaven");
        assert_eq!(slug("Polda 2 – Příběh ze života"), "polda-2-pribeh-ze-zivota");
        assert_eq!(slug("  --  "), "");
        assert_eq!(roman_slug("baldurs-gate-3"), "baldurs-gate-iii");
        assert_eq!(roman_slug("heroes-of-might-and-magic-3"), "heroes-of-might-and-magic-iii");
        assert_eq!(roman_slug("fallout-1"), "fallout-1", "1 stays");
        let v = slug_variants("Baldurs Gate 3");
        assert_eq!(v[..2], ["baldurs-gate-3".to_string(), "baldurs-gate-iii".to_string()]);
        let v = slug_variants("The Witcher: Enhanced Edition Director's Cut");
        assert_eq!(v[0], "the-witcher-enhanced-edition-directors-cut");
        assert!(v.contains(&"the-witcher".to_string()), "{v:?}");
        let v = slug_variants("Fallout: A Post Nuclear Role Playing Game");
        assert!(v.contains(&"fallout".to_string()), "{v:?}");
        assert!(v.len() <= 5);
    }

    #[test]
    fn pasted_page_addresses_are_recognised() {
        assert_eq!(slug_from_input("https://www.databaze-her.cz/hry/system-shock-2/").as_deref(), Some("system-shock-2"));
        assert_eq!(slug_from_input(" databaze-her.cz/hry/the-witcher-3-wild-hunt/komentare/?razeni=0 ").as_deref(), Some("the-witcher-3-wild-hunt"));
        assert_eq!(slug_from_input("https://www.databaze-her.cz/hry/Gothic#top").as_deref(), Some("gothic"));
        assert_eq!(slug_from_input("System Shock 2"), None);
        assert_eq!(slug_from_input("https://www.databaze-her.cz/vyvojari/irrational-games/"), None);
        assert_eq!(slug_from_input("https://www.databaze-her.cz/hry/../x"), None);
        assert_eq!(slug_from_input("https://example.com/hry/x/"), None);
    }

    /// The shape of a real game page (www.databaze-her.cz/hry/the-witcher-3-wild-hunt/), shortened.
    pub const PAGE: &str = r##"<!DOCTYPE html><html lang="cs"><head><title>The Witcher 3: Wild Hunt – Databáze-her.cz</title></head><body>
<script type="application/ld+json">
	{
		"@context": "http://schema.org/",
		"@type": "VideoGame",
		"name": "The Witcher 3: Wild Hunt",
		"description": "Geralt z Rivie se nachází…",
		"genre": [ "RPG" ],
		"author": [ {
			"@type": "Organization",
			"name": "CD Projekt RED",
			"url": "https://www.databaze-her.cz/vyvojari/cd-projekt-red/"
		}, {
			"@type": "Organization",
			"name": "Saber Interactive",
			"url": "https://www.databaze-her.cz/vyvojari/saber-interactive/"
		} ]
	}
</script>
<header id="game-header"><div class="game-title">
	<h1>The Witcher 3: Wild Hunt</h1>
	<h4>Zaklínač 3: Divoký hon, Wiedźmin 3: Dziki Gon</h4> </div>
<div class="game-developers">
	<span><a href="/vyvojari/cd-projekt-red/">CD Projekt RED</a><span class="text-separator">&nbsp;&bull;&nbsp;</span></span>
	<span><a href="/vyvojari/saber-interactive/">Saber Interactive</a> (Switch)</span>
</div>
<div class="game-dates"><div>
	<div class="game-date">
		<div class="platforms -inline"><a href="/platformy/playstation-4/" title="PlayStation 4" class="platform" data-system-name="playstation-4">PS4</a></div>
		<div>12.05.2015</div></div>
	<div class="game-date">
		<div class="platforms -inline"><a href="/platformy/pc/" title="PC" class="platform" data-system-name="pc">PC</a><a href="/platformy/xbox-one/" class="platform" data-system-name="xbox-one">XOne</a></div>
		<div>19.05.2015</div></div>
	<div class="game-date">
		<div class="platforms -inline"><a href="/platformy/nintendo-switch/" class="platform" data-system-name="nintendo-switch">Switch</a></div>
		<div>15.10.2019</div></div>
	</div><script>dh(function(){});</script></div>
</header>
<dl class="game-genres details"><dt>Žánr</dt><dd>
	<div><a href="/hry/?zanr=p7p">RPG</a> &gt; <a href="/hry/?zanr=s27s">nezařazeno</a> <span title="primární žánr"></span></div>
	<div class="grid -cols-3"><div><a href="/hry/?zanr=p1p">akce</a> &gt; <a href="/hry/?zanr=s3s">3rd person akce</a></div></div>
</dd></dl>
<div class="grid -cols-3"><div><dl class="details"><dt>Forma</dt><dd>placená hra</dd></dl></div>
<div><dl class="details"><dt>Rozsah</dt><dd>samostatná hra</dd></dl></div></div>
<div class="game-description article-text">Geralt z Rivie se nachází ve skutečně <a href="/x/">nezáviděníhodné</a> situaci.\r<br>\r<br>V roce 2026 vyšla verze <i>Remastered</i> &ndash; s novou grafikou.</div>
<div class="store -profile"><div>nic</div></div>
</body></html>"##;

    #[test]
    fn a_game_page_is_read() {
        let page = PAGE.replace("\\r", "\r");
        let c = parse_page(&page, "the-witcher-3-wild-hunt").unwrap();
        assert_eq!(c.title, "The Witcher 3: Wild Hunt");
        assert_eq!(c.alt_titles, ["Zaklínač 3: Divoký hon", "Wiedźmin 3: Dziki Gon"]);
        assert_eq!(c.developers, ["CD Projekt RED", "Saber Interactive"]);
        assert_eq!(c.genres, ["RPG"], "the page data names the genres");
        assert_eq!(c.tags, ["3rd person akce"], "\"nezařazeno\" (unclassified) is no tag");
        assert_eq!(c.year, Some(2015), "the PC release");
        assert_eq!(c.product_type, "game");
        assert_eq!(c.store_link, "https://www.databaze-her.cz/hry/the-witcher-3-wild-hunt/");
        assert_eq!(c.description.as_deref(), Some("Geralt z Rivie se nachází ve skutečně nezáviděníhodné situaci.\n\nV roce 2026 vyšla verze Remastered \u{2013} s novou grafikou."));
        assert_eq!(c.cover, None, "covers are not taken from Databáze-her.cz");

        // without the page data: the header and the genre list
        let plain = page.replace("application/ld+json", "text/plain");
        let c = parse_page(&plain, "x").unwrap();
        assert_eq!(c.developers, ["CD Projekt RED", "Saber Interactive"]);
        assert_eq!(c.genres, ["RPG", "Akce"]);
        // an add-on
        let dlc = page.replace("samostatná hra", "datadisk");
        assert_eq!(parse_page(&dlc, "x").unwrap().product_type, "dlc");
        // no PC release: the first one
        assert_eq!(release_year("<div class=\"game-date\">PS5 <div>14.12.2022</div></div><div class=\"game-date\">X <div>2019</div></div>"), Some(2019));
        assert_eq!(release_year("nothing"), None);
        // not a game page
        assert!(parse_page("<html><body><h1>Stránka nenalezena</h1></body></html>", "x").is_none());
    }
}
