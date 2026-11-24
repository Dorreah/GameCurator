//! Game details from the internet: what the three sources share.
//!
//! The only parts of Game Curator that go online - and only when the user
//! presses "Fetch details" on the Archive Scanner page, for the source they
//! chose:
//!
//! * **GOG.com** (`gog.rs`) - the official title, description, genres, tags,
//!   developer, publisher, year and (for games without one) the cover;
//! * **Databáze-her.cz** (`dbher.rs`) - a Czech description, genres,
//!   developer and year from the public game pages (content under
//!   CC BY-NC-ND 3.0 CZ: the source is written under the description);
//! * **MobyGames** (`moby.rs`) - an English description, genres, developer,
//!   publisher and year through the MobyGames API, with the user's own key.
//!
//! Nothing is saved before the user has reviewed it, filled fields are only
//! replaced when the user ticks them, and the game's own files are never
//! touched. Each source may contact only its own addresses, over HTTPS; the
//! only things sent are the titles looked up (and the MobyGames key to
//! MobyGames). No account, no cookies.

use std::io::Read;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::archive::ArchiveLayout;
use crate::{db, games, images, text, CoreError, Result};

// ---------------------------------------------------------------- sources

/// Where details come from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Source {
    #[default]
    Gog,
    Dbher,
    Moby,
}

impl Source {
    pub const ALL: [Source; 3] = [Source::Gog, Source::Dbher, Source::Moby];

    /// The name shown to the user.
    pub fn site(self) -> &'static str {
        match self {
            Source::Gog => "GOG.com",
            Source::Dbher => "Databáze-her.cz",
            Source::Moby => "MobyGames",
        }
    }

    /// The key used by the page ("gog", "dbher", "moby").
    pub fn key(self) -> &'static str {
        match self {
            Source::Gog => "gog",
            Source::Dbher => "dbher",
            Source::Moby => "moby",
        }
    }

    pub fn from_key(key: &str) -> Option<Source> {
        Source::ALL.into_iter().find(|s| s.key() == key)
    }

    /// The hosts this source may contact over HTTPS (a leading "." also
    /// allows the subdomains).
    fn hosts(self) -> &'static [&'static str] {
        match self {
            Source::Gog => &["gog.com", ".gog.com", "gog-statics.com", ".gog-statics.com"],
            Source::Dbher => &["databaze-her.cz", "www.databaze-her.cz"],
            Source::Moby => &["api.mobygames.com"],
        }
    }

    /// A pause between two requests, to go easy on the site (MobyGames
    /// allows at most one request per second).
    pub fn pause(self) -> Duration {
        match self {
            Source::Gog => Duration::from_millis(150),
            Source::Dbher => Duration::from_millis(1000),
            Source::Moby => Duration::from_millis(1100),
        }
    }
}

/// Is this address one the source may contact? HTTPS to its own hosts only
/// (plus the local test server when allowed).
pub fn allowed_url(source: Source, allow_local_http: bool, raw: &str) -> bool {
    let Ok(u) = url::Url::parse(raw) else { return false };
    let Some(host) = u.host_str().map(str::to_ascii_lowercase) else { return false };
    if !u.username().is_empty() || u.password().is_some() {
        return false;
    }
    match u.scheme() {
        "https" => u.port().is_none() && source.hosts().iter().any(|h| match h.strip_prefix('.') {
            Some(domain) => host.ends_with(&format!(".{domain}")),
            None => host == *h,
        }),
        "http" => allow_local_http && (host == "127.0.0.1" || host == "localhost"),
        _ => false,
    }
}

// ---------------------------------------------------------------- errors

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NetError {
    /// The site could not be reached (no internet, blocked, timeout).
    Offline(String),
    /// Too many requests: the site asks to slow down.
    RateLimited,
    /// Not found (HTTP 404).
    NotFound,
    /// The site refused the request (HTTP 401/403) - for MobyGames: the key.
    Unauthorized,
    /// MobyGames needs the user's API key and none is set.
    NoKey,
    /// Another HTTP status or an unreadable answer.
    Bad(String),
    /// An address outside the source (never contacted).
    NotAllowed(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LookupError {
    pub source: Source,
    pub kind: NetError,
}

impl LookupError {
    pub fn new(source: Source, kind: NetError) -> Self {
        LookupError { source, kind }
    }
    /// The user may need to act (no internet, too fast, the key): looking
    /// further games up would fail the same way.
    pub fn stops_all(&self) -> bool {
        matches!(self.kind, NetError::Offline(_) | NetError::RateLimited | NetError::Unauthorized | NetError::NoKey)
    }
}

impl std::fmt::Display for LookupError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let site = self.source.site();
        match &self.kind {
            NetError::Offline(e) => write!(f, "{site} could not be reached. Is the computer connected to the internet? ({e})"),
            NetError::RateLimited if self.source == Source::Moby => {
                write!(f, "MobyGames asks to slow down (it allows 360 requests an hour). Please try again later.")
            }
            NetError::RateLimited => write!(f, "{site} asks to slow down. Please try again in a while."),
            NetError::NotFound => write!(f, "Not found on {site}."),
            NetError::Unauthorized if self.source == Source::Moby => {
                write!(f, "MobyGames did not accept the API key. Please check it in Settings.")
            }
            NetError::Unauthorized => write!(f, "{site} refused the request."),
            NetError::NoKey => write!(f, "Enter your MobyGames API key in Settings first."),
            NetError::Bad(e) => write!(f, "{site} sent an unexpected answer ({e})."),
            NetError::NotAllowed(u) => write!(f, "Game Curator only contacts {site}, not {u}."),
        }
    }
}

impl From<LookupError> for CoreError {
    fn from(e: LookupError) -> Self {
        CoreError::Invalid(e.to_string())
    }
}

pub type LookupResult<T> = std::result::Result<T, LookupError>;

/// Hides the MobyGames key in anything that could be shown or logged.
pub fn redact(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find("api_key=") {
        out.push_str(&rest[..i + 8]);
        out.push('\u{2026}');
        rest = &rest[i + 8..];
        let end = rest.find(|c: char| c == '&' || c.is_whitespace() || c == '"' || c == ')').unwrap_or(rest.len());
        rest = &rest[end..];
    }
    out.push_str(rest);
    out
}

// ---------------------------------------------------------------- fetching

const USER_AGENT: &str = concat!("GameCurator/", env!("CARGO_PKG_VERSION"), " (offline DRM-free game archive)");
pub const MAX_TEXT: u64 = 8 * 1024 * 1024;
pub const MAX_IMAGE: u64 = 15 * 1024 * 1024;

/// HTTP GET for one source: every address (also after a redirect) is
/// checked first, requests are spaced by the source's pause.
pub struct Fetcher {
    agent: ureq::Agent,
    source: Source,
    allow_local_http: bool,
    last: Mutex<Option<Instant>>,
}

impl Fetcher {
    pub fn new(source: Source, allow_local_http: bool) -> Self {
        let agent = ureq::AgentBuilder::new()
            .timeout_connect(Duration::from_secs(10))
            .timeout(Duration::from_secs(30))
            .redirects(0)
            .user_agent(USER_AGENT)
            .build();
        Fetcher { agent, source, allow_local_http, last: Mutex::new(None) }
    }

    pub fn source(&self) -> Source {
        self.source
    }

    pub fn allows(&self, url: &str) -> bool {
        allowed_url(self.source, self.allow_local_http, url)
    }

    pub fn err(&self, kind: NetError) -> LookupError {
        LookupError::new(self.source, kind)
    }

    fn wait_turn(&self) {
        let pause = self.source.pause();
        let mut last = self.last.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(t) = *last {
            let since = t.elapsed();
            if since < pause {
                std::thread::sleep(pause - since);
            }
        }
        *last = Some(Instant::now());
    }

    pub fn get(&self, url: &str, max: u64) -> LookupResult<Vec<u8>> {
        let mut url = url.to_string();
        for _ in 0..4 {
            if !self.allows(&url) {
                return Err(self.err(NetError::NotAllowed(redact(&url))));
            }
            self.wait_turn();
            let resp = match self.agent.get(&url).set("Accept", "application/json, text/html;q=0.9, */*;q=0.5").call() {
                Ok(r) => r,
                Err(ureq::Error::Status(429, _)) => return Err(self.err(NetError::RateLimited)),
                Err(ureq::Error::Status(404, _)) | Err(ureq::Error::Status(410, _)) => return Err(self.err(NetError::NotFound)),
                Err(ureq::Error::Status(401, _)) | Err(ureq::Error::Status(403, _)) => return Err(self.err(NetError::Unauthorized)),
                Err(ureq::Error::Status(code, _)) => return Err(self.err(NetError::Bad(format!("HTTP {code}")))),
                Err(ureq::Error::Transport(t)) => return Err(self.err(NetError::Offline(redact(&t.to_string())))),
            };
            if (300..400).contains(&resp.status()) {
                let next = resp.header("location").ok_or_else(|| self.err(NetError::Bad("redirect without address".into())))?;
                url = url::Url::parse(&url).and_then(|b| b.join(next)).map_err(|e| self.err(NetError::Bad(e.to_string())))?.to_string();
                continue;
            }
            let mut buf = Vec::new();
            resp.into_reader().take(max + 1).read_to_end(&mut buf).map_err(|e| self.err(NetError::Offline(redact(&e.to_string()))))?;
            if buf.len() as u64 > max {
                return Err(self.err(NetError::Bad("answer too large".into())));
            }
            return Ok(buf);
        }
        Err(self.err(NetError::Bad("too many redirects".into())))
    }

    pub fn get_text(&self, url: &str) -> LookupResult<String> {
        String::from_utf8(self.get(url, MAX_TEXT)?).map_err(|_| self.err(NetError::Bad("not text".into())))
    }
}

// ---------------------------------------------------------------- products

/// A product (a game page) found at a source.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Candidate {
    pub id: String,
    pub title: String,
    pub year: Option<i64>,
    pub developers: Vec<String>,
    pub publishers: Vec<String>,
    pub genres: Vec<String>,
    pub tags: Vec<String>,
    /// "game", "pack" (an edition or bundle) ...
    pub product_type: String,
    /// The product's page at the source.
    pub store_link: String,
    /// Address of the vertical cover image.
    #[serde(skip)]
    pub cover: Option<String>,
    /// How well the title matches the one looked up (0-100).
    pub score: u8,
    /// The description, when the source gives it with the product
    /// (Databáze-her.cz, MobyGames); GOG's is fetched separately.
    #[serde(skip)]
    pub description: Option<String>,
    /// Other names of the game (translations, re-releases) - they count when
    /// titles are matched.
    #[serde(skip)]
    pub alt_titles: Vec<String>,
    /// MobyGames: the platform whose release names the companies.
    #[serde(skip)]
    pub platform: Option<i64>,
}

impl Candidate {
    pub fn new(id: impl Into<String>, title: impl Into<String>) -> Self {
        Candidate {
            id: id.into(),
            title: title.into(),
            year: None,
            developers: Vec::new(),
            publishers: Vec::new(),
            genres: Vec::new(),
            tags: Vec::new(),
            product_type: "game".into(),
            store_link: String::new(),
            cover: None,
            score: 0,
            description: None,
            alt_titles: Vec::new(),
            platform: None,
        }
    }
}

pub(crate) fn str_list(v: &serde_json::Value, key: &str) -> Vec<String> {
    v.get(key)
        .and_then(|a| a.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|x| x.as_str().map(str::to_string).or_else(|| x.get("name").and_then(|n| n.as_str()).map(str::to_string)))
                .map(|s| text::tidy(&s))
                .filter(|s| !s.is_empty())
                .collect()
        })
        .unwrap_or_default()
}

pub(crate) fn year_of(date: &str) -> Option<i64> {
    let y: i64 = date.get(..4)?.parse().ok()?;
    (1940..=2100).contains(&y).then_some(y)
}


// ---------------------------------------------------------------- matching titles

/// Lower case, no diacritics, no ™®©, "&" = "and", words only.
pub fn normalize(title: &str) -> String {
    let s = text::search_key(&title.replace('&', " and "));
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if c.is_alphanumeric() {
            out.push(c);
        } else if c == '\'' || c == '\u{2019}' {
            // "Baldur's" = "Baldurs"
        } else {
            out.push(' ');
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Words that name an edition rather than the game.
const EDITION: &[&str] = &[
    "game of the year edition",
    "game of the year",
    "goty edition",
    "goty",
    "enhanced edition",
    "definitive edition",
    "complete edition",
    "complete pack",
    "complete",
    "gold edition",
    "gold",
    "special edition",
    "deluxe edition",
    "ultimate edition",
    "anniversary edition",
    "collectors edition",
    "directors cut",
    "director s cut",
    "remastered",
    "classic",
    "edition",
];

/// The title without "The" in front and without edition words at the end.
pub fn core_title(title: &str) -> String {
    let mut s = normalize(title);
    loop {
        let before = s.clone();
        for w in EDITION {
            if let Some(rest) = s.strip_suffix(w) {
                if rest.is_empty() || rest.ends_with(' ') {
                    s = rest.trim_end().to_string();
                    break;
                }
            }
        }
        if s == before {
            break;
        }
    }
    let s = s.strip_prefix("the ").map(str::to_string).unwrap_or(s);
    if s.is_empty() {
        normalize(title)
    } else {
        s
    }
}

/// How well two titles match (0-100): 100 the same, 92 the same apart from
/// "The" and the edition, then by the words they share.
pub fn score(ours: &str, theirs: &str) -> u8 {
    let (a, b) = (normalize(ours), normalize(theirs));
    if a.is_empty() || b.is_empty() {
        return 0;
    }
    if a == b {
        return 100;
    }
    let (ca, cb) = (core_title(ours), core_title(theirs));
    if ca == cb {
        return 92;
    }
    let wa: Vec<&str> = ca.split(' ').collect();
    let wb: Vec<&str> = cb.split(' ').collect();
    // one is the other plus a subtitle ("Fallout" / "Fallout: A Post Nuclear ...")
    if wb.starts_with(&wa) || wa.starts_with(&wb) {
        let (short, long) = if wa.len() < wb.len() { (wa.len(), wb.len()) } else { (wb.len(), wa.len()) };
        return (60 + 25 * short / long) as u8;
    }
    let common = wa.iter().filter(|w| wb.contains(w)).count();
    let all = wa.len() + wb.len() - common;
    (70 * common / all.max(1)) as u8
}

/// Sorts the products by how well they match `title` (games before packs
/// when equally good) and fills in their scores.
pub fn rank(title: &str, cands: Vec<Candidate>) -> Vec<Candidate> {
    rank_any(&[title], cands)
}

/// The same, scoring each product by the best of several titles (the game's
/// title in the catalog and the words the user searched for).
pub fn rank_any(titles: &[&str], mut cands: Vec<Candidate>) -> Vec<Candidate> {
    for c in &mut cands {
        let names: Vec<&String> = std::iter::once(&c.title).chain(c.alt_titles.iter()).collect();
        c.score = titles.iter().flat_map(|t| names.iter().map(move |n| score(t, n))).max().unwrap_or(0);
    }
    cands.sort_by(|a, b| b.score.cmp(&a.score).then_with(|| (a.product_type != "game").cmp(&(b.product_type != "game"))));
    cands
}

/// A match good enough to be offered without asking: the best product has
/// the same title (apart from "The" and the edition) and no other product of
/// the same kind matches as well.
pub fn confident(ranked: &[Candidate]) -> bool {
    match ranked {
        [] => false,
        [best, rest @ ..] => best.score >= 90 && !rest.iter().any(|c| c.score == best.score && c.product_type == best.product_type),
    }
}

// ---------------------------------------------------------------- descriptions

pub(crate) fn entity(name: &str) -> Option<char> {
    Some(match name {
        "amp" => '&',
        "lt" => '<',
        "gt" => '>',
        "quot" => '"',
        "apos" | "#39" => '\'',
        "nbsp" => ' ',
        "ndash" => '\u{2013}',
        "mdash" => '\u{2014}',
        "hellip" => '\u{2026}',
        "lsquo" => '\u{2018}',
        "rsquo" => '\u{2019}',
        "ldquo" => '\u{201c}',
        "rdquo" => '\u{201d}',
        "trade" => '\u{2122}',
        "reg" => '\u{ae}',
        "copy" => '\u{a9}',
        "bull" => '\u{2022}',
        "eacute" => '\u{e9}',
        _ => {
            let n = name.strip_prefix('#')?;
            let code = match n.strip_prefix('x').or_else(|| n.strip_prefix('X')) {
                Some(hex) => u32::from_str_radix(hex, 16).ok()?,
                None => n.parse().ok()?,
            };
            return char::from_u32(code);
        }
    })
}

/// Plain text of a description (HTML): paragraphs and list items keep
/// their own lines, pictures and markup are left out.
pub fn html_to_text(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut chars = html.chars().peekable();
    let mut skip_until: Option<&'static str> = None;
    while let Some(c) = chars.next() {
        let starts_tag = c == '<' && chars.peek().is_some_and(|n| n.is_ascii_alphabetic() || *n == '/' || *n == '!');
        match c {
            '<' if starts_tag => {
                let mut tag = String::new();
                for t in chars.by_ref() {
                    if t == '>' {
                        break;
                    }
                    tag.push(t);
                }
                let closing = tag.starts_with('/');
                let name: String = tag.trim_start_matches('/').chars().take_while(|c| c.is_ascii_alphanumeric()).collect::<String>().to_ascii_lowercase();
                if let Some(end) = skip_until {
                    if closing && name == end {
                        skip_until = None;
                    }
                    continue;
                }
                match name.as_str() {
                    "script" | "style" if !closing => skip_until = Some(if name == "script" { "script" } else { "style" }),
                    "br" => out.push('\n'),
                    "li" if !closing => out.push_str("\n\u{2022} "),
                    // a list item ends where the next begins or the list ends
                    "p" | "div" | "ul" | "ol" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6" | "hr" | "table" | "tr" | "blockquote" | "section" => {
                        out.push_str("\n\n")
                    }
                    _ => {}
                }
            }
            '&' if skip_until.is_none() => {
                let mut name = String::new();
                while let Some(&n) = chars.peek() {
                    if n == ';' || name.len() > 10 || !(n.is_ascii_alphanumeric() || n == '#') {
                        break;
                    }
                    name.push(n);
                    chars.next();
                }
                if chars.peek() == Some(&';') {
                    if let Some(ch) = entity(&name) {
                        chars.next();
                        out.push(ch);
                        continue;
                    }
                }
                out.push('&');
                out.push_str(&name);
            }
            c if skip_until.is_some() => {
                let _ = c;
            }
            '\n' | '\r' | '\t' => out.push(' '),
            c => out.push(c),
        }
    }
    // tidy: spaces inside lines, at most one empty line in a row
    let mut lines: Vec<String> = Vec::new();
    for line in out.split('\n') {
        let l = line.split_whitespace().collect::<Vec<_>>().join(" ");
        if l.is_empty() && lines.last().map_or(true, |p: &String| p.is_empty()) {
            continue;
        }
        if l == "\u{2022}" {
            continue;
        }
        lines.push(l);
    }
    while lines.last().is_some_and(|l| l.is_empty()) {
        lines.pop();
    }
    lines.join("\n")
}

pub(crate) fn cut(s: &str, max_chars: usize) -> String {
    match s.char_indices().nth(max_chars) {
        Some((i, _)) => s[..i].trim_end().to_string(),
        None => s.to_string(),
    }
}

/// Joins names ("Larian Studios, Feral Interactive") within `max` characters.
pub(crate) fn join_within(names: &[String], max: usize) -> String {
    let mut out = String::new();
    for n in names {
        let next = if out.is_empty() { n.clone() } else { format!("{out}, {n}") };
        if next.chars().count() > max {
            break;
        }
        out = next;
    }
    if out.is_empty() {
        names.first().map(|n| cut(n, max)).unwrap_or_default()
    } else {
        out
    }
}

// ---------------------------------------------------------------- cover previews

pub(crate) fn base64(bytes: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        out.push(T[(n >> 18) as usize & 63] as char);
        out.push(T[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 { T[(n >> 6) as usize & 63] as char } else { '=' });
        out.push(if chunk.len() > 2 { T[n as usize & 63] as char } else { '=' });
    }
    out
}

/// A small PNG of a cover for the review, as a `data:` address (the page
/// shows no pictures from the internet). `None` if it is not a picture.
pub fn cover_preview(bytes: &[u8]) -> Option<String> {
    let mut reader = image::ImageReader::new(std::io::Cursor::new(bytes)).with_guessed_format().ok()?;
    if !matches!(reader.format(), Some(image::ImageFormat::Jpeg | image::ImageFormat::Png | image::ImageFormat::WebP)) {
        return None;
    }
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(16_000);
    limits.max_image_height = Some(16_000);
    limits.max_alloc = Some(512 * 1024 * 1024);
    reader.limits(limits);
    let small = reader.decode().ok()?.thumbnail(96, 128);
    let mut out = std::io::Cursor::new(Vec::new());
    small.write_to(&mut out, image::ImageFormat::Png).ok()?;
    Some(format!("data:image/png;base64,{}", base64(&out.into_inner())))
}

// ---------------------------------------------------------------- proposals

/// The details of a game as they are in the catalog now.
#[derive(Debug, Clone, Default)]
pub struct Current {
    pub title: String,
    pub description: String,
    pub genre: String,
    pub developer: String,
    pub publisher: String,
    pub year: Option<i64>,
    pub tags: Vec<String>,
    pub has_artwork: bool,
}

pub fn current(conn: &Connection, game_id: i64) -> Result<Current> {
    let mut c = conn
        .query_row(
            "SELECT title, description, genre, developer, publisher, release_year, artwork_path IS NOT NULL FROM games WHERE id = ?1",
            [game_id],
            |r| {
                Ok(Current {
                    title: r.get(0)?,
                    description: r.get(1)?,
                    genre: r.get(2)?,
                    developer: r.get(3)?,
                    publisher: r.get(4)?,
                    year: r.get(5)?,
                    tags: Vec::new(),
                    has_artwork: r.get(6)?,
                })
            },
        )
        .optional()?
        .ok_or_else(|| CoreError::not_found("The game no longer exists."))?;
    c.tags = games::game_tags(conn, game_id)?;
    Ok(c)
}

/// What a source knows about a product, ready to be offered.
#[derive(Debug, Clone)]
pub struct Details {
    pub source: Source,
    pub product: Candidate,
    pub description: Option<String>,
    pub has_cover: bool,
}

/// Longest description kept.
const MAX_DESCRIPTION: usize = 20_000;

impl Details {
    fn title(&self) -> String {
        cut(&self.product.title, 200)
    }
    /// Where the description comes from, written under it: Databáze-her.cz
    /// asks for the author and a link to the page (CC BY-NC-ND 3.0 CZ),
    /// MobyGames gets the same courtesy.
    pub fn attribution(&self) -> Option<String> {
        let page = self.product.store_link.trim();
        match self.source {
            Source::Gog => None,
            Source::Dbher => Some(format!(
                "\u{2014} Zdroj: Databáze-her.cz{} (licence CC BY-NC-ND 3.0 CZ)",
                if page.is_empty() { String::new() } else { format!(", {page}") }
            )),
            Source::Moby => Some(format!("\u{2014} Source: MobyGames{}", if page.is_empty() { String::new() } else { format!(", {page}") })),
        }
    }
    fn description(&self) -> String {
        let Some(d) = self.description.as_deref().map(str::trim).filter(|d| !d.is_empty()) else { return String::new() };
        match self.attribution() {
            Some(a) => format!("{}\n\n{a}", cut(d, MAX_DESCRIPTION - a.chars().count() - 2)),
            None => cut(d, MAX_DESCRIPTION),
        }
    }
    fn genre(&self) -> String {
        join_within(&self.product.genres, 100)
    }
    fn developer(&self) -> String {
        join_within(&self.product.developers, 200)
    }
    fn publisher(&self) -> String {
        join_within(&self.product.publishers, 200)
    }
    /// Tags the game does not have yet (not repeating its genres), at most
    /// 10, and never more than 30 tags in all.
    fn new_tags(&self, have: &[String]) -> Vec<String> {
        let lower = |s: &str| s.to_lowercase();
        let mut out: Vec<String> = Vec::new();
        for t in &self.product.tags {
            let t = text::tidy(t);
            if t.is_empty() || t.chars().count() > 40 || t.contains(',') {
                continue;
            }
            let l = lower(&t);
            if have.iter().any(|h| lower(h) == l) || self.product.genres.iter().any(|g| lower(g) == l) || out.iter().any(|o| lower(o) == l) {
                continue;
            }
            if out.len() >= 10 || have.len() + out.len() >= 30 {
                break;
            }
            out.push(t);
        }
        out
    }
}

/// One detail a source offers for a game.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Change {
    /// "title", "description", "genre", "tags", "developer", "publisher", "year" or "cover"
    pub field: String,
    pub current: String,
    pub proposed: String,
    /// Ticked in the review: only empty fields are, so nothing the user wrote
    /// is replaced unless they tick it.
    pub selected: bool,
}

fn same(a: &str, b: &str) -> bool {
    a.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase() == b.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase()
}

/// The changes the source's details would bring (fields that would stay the
/// same are left out).
pub fn propose(cur: &Current, d: &Details) -> Vec<Change> {
    fn text_field(out: &mut Vec<Change>, field: &str, current: &str, proposed: String) {
        if !proposed.is_empty() && !same(current, &proposed) {
            out.push(Change { field: field.into(), current: current.to_string(), proposed, selected: current.trim().is_empty() });
        }
    }
    let mut out = Vec::new();
    text_field(&mut out, "title", &cur.title, d.title());
    text_field(&mut out, "description", &cur.description, d.description());
    text_field(&mut out, "genre", &cur.genre, d.genre());
    let tags = d.new_tags(&cur.tags);
    if !tags.is_empty() {
        out.push(Change { field: "tags".into(), current: cur.tags.join(", "), proposed: tags.join(", "), selected: cur.tags.is_empty() });
    }
    text_field(&mut out, "developer", &cur.developer, d.developer());
    text_field(&mut out, "publisher", &cur.publisher, d.publisher());
    if let Some(y) = d.product.year {
        if cur.year != Some(y) {
            out.push(Change {
                field: "year".into(),
                current: cur.year.map(|y| y.to_string()).unwrap_or_default(),
                proposed: y.to_string(),
                selected: cur.year.is_none(),
            });
        }
    }
    if !cur.has_artwork && d.has_cover {
        out.push(Change { field: "cover".into(), current: String::new(), proposed: "Cover from GOG.com".into(), selected: true });
    }
    out
}

pub const FIELDS: &[&str] = &["title", "description", "genre", "tags", "developer", "publisher", "year", "cover"];

/// Saves the chosen details. The values come from the source's data held
/// here, not from the page; a cover is only added to a game that still has none.
/// Returns the fields that were saved.
pub fn apply(conn: &mut Connection, layout: &ArchiveLayout, game_id: i64, d: &Details, fields: &[String], cover: Option<&[u8]>) -> Result<Vec<String>> {
    if let Some(f) = fields.iter().find(|f| !FIELDS.contains(&f.as_str())) {
        return Err(CoreError::invalid(format!("Unknown detail \u{201c}{f}\u{201d}.")));
    }
    let cur = current(conn, game_id)?;
    let want = |f: &str| fields.iter().any(|x| x == f);

    // the cover is converted before anything is written to the database
    let mut new_art: Option<images::ProcessedArtwork> = None;
    if want("cover") && !cur.has_artwork {
        if let Some(bytes) = cover {
            new_art = Some(images::process_artwork_bytes(layout, bytes)?);
        }
    }
    let result = (|| -> Result<Vec<String>> {
        let mut done: Vec<String> = Vec::new();
        let tx = conn.transaction()?;
        let now = db::now_iso(&tx)?;
        let set = |done: &mut Vec<String>, sql: &str, value: &dyn rusqlite::ToSql, name: &str| -> Result<()> {
            tx.execute(&format!("UPDATE games SET {sql}, date_modified = ?2 WHERE id = ?3"), params![value, now, game_id])?;
            done.push(name.to_string());
            Ok(())
        };
        if want("title") && !d.title().is_empty() {
            let t = d.title();
            set(&mut done, "title = ?1", &t, "title")?;
            tx.execute("UPDATE games SET title_search = ?1 WHERE id = ?2", params![text::search_key(&t), game_id])?;
        }
        if want("description") && !d.description().is_empty() {
            set(&mut done, "description = ?1", &d.description(), "description")?;
        }
        if want("genre") && !d.genre().is_empty() {
            set(&mut done, "genre = ?1", &d.genre(), "genre")?;
        }
        if want("developer") && !d.developer().is_empty() {
            set(&mut done, "developer = ?1", &d.developer(), "developer")?;
        }
        if want("publisher") && !d.publisher().is_empty() {
            set(&mut done, "publisher = ?1", &d.publisher(), "publisher")?;
        }
        if want("year") {
            if let Some(y) = d.product.year {
                set(&mut done, "release_year = ?1", &y, "year")?;
            }
        }
        if let Some(a) = &new_art {
            tx.execute(
                "UPDATE games SET artwork_path = ?1, thumbnail_path = ?2, cover_crop = NULL, date_modified = ?3 WHERE id = ?4 AND artwork_path IS NULL",
                params![a.artwork_path, a.thumbnail_path, now, game_id],
            )?;
            done.push("cover".into());
        }
        if want("tags") {
            let add = d.new_tags(&cur.tags);
            if !add.is_empty() {
                let mut all = cur.tags.clone();
                all.extend(add);
                let all = games::normalize_tags(&all)?;
                games::set_tags(&tx, game_id, &all)?;
                tx.execute("UPDATE games SET date_modified = ?1 WHERE id = ?2", params![now, game_id])?;
                done.push("tags".into());
            }
        }
        tx.commit()?;
        Ok(done)
    })();
    if result.is_err() {
        if let Some(a) = &new_art {
            images::remove_artwork_files(layout, Some(&a.artwork_path), Some(&a.thumbnail_path));
        }
    }
    result
}

/// Games whose details are not complete yet (any of description, genre,
/// developer, publisher, year, tags or cover missing).
pub fn games_missing_details(conn: &Connection) -> Result<Vec<i64>> {
    let mut stmt = conn.prepare(
        "SELECT id FROM games g WHERE trim(description) = '' OR trim(genre) = '' OR trim(developer) = '' OR trim(publisher) = ''
            OR release_year IS NULL OR artwork_path IS NULL OR NOT EXISTS (SELECT 1 FROM game_tags t WHERE t.game_id = g.id)
         ORDER BY title COLLATE NOCASE",
    )?;
    let ids = stmt.query_map([], |r| r.get(0))?.collect::<rusqlite::Result<Vec<i64>>>()?;
    Ok(ids)
}

pub fn all_games(conn: &Connection) -> Result<Vec<i64>> {
    let mut stmt = conn.prepare("SELECT id FROM games ORDER BY title COLLATE NOCASE")?;
    let ids = stmt.query_map([], |r| r.get(0))?.collect::<rusqlite::Result<Vec<i64>>>()?;
    Ok(ids)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn titles_match_without_the_editions_and_punctuation() {
        assert_eq!(normalize("Baldur's Gate™: Enhanced Edition"), "baldurs gate enhanced edition");
        assert_eq!(normalize("Zaklínač & Spol."), "zaklinac and spol");
        assert_eq!(core_title("The Witcher: Enhanced Edition Director's Cut"), "witcher");
        assert_eq!(core_title("Edition"), "edition", "never empty");
        assert_eq!(score("Baldurs Gate 3", "Baldur's Gate 3"), 100);
        assert_eq!(score("The Witcher Enhanced Edition", "The Witcher: Enhanced Edition Director's Cut"), 92);
        assert_eq!(score("Witcher 3 Wild Hunt", "The Witcher 3: Wild Hunt - Game of the Year Edition"), 92);
        assert!(score("Fallout", "Fallout: A Post Nuclear Role Playing Game") >= 60);
        assert!(score("Fallout", "Fallout 2") < 90);
        assert!(score("System Shock 2", "Disco Elysium") < 20);
        assert_eq!(score("", "x"), 0);
    }

    fn cand(id: &str, title: &str, kind: &str) -> Candidate {
        Candidate { product_type: kind.into(), ..Candidate::new(id, title) }
    }

    #[test]
    fn the_best_product_comes_first_and_ties_need_a_look() {
        let r = rank("The Witcher 3: Wild Hunt", vec![cand("1", "The Witcher 3: Wild Hunt - Complete Edition", "pack"), cand("2", "The Witcher 3: Wild Hunt", "game"), cand("3", "Thronebreaker", "game")]);
        assert_eq!(r[0].id, "2");
        assert!(confident(&r));
        let tie = rank("Gothic", vec![cand("1", "Gothic", "game"), cand("2", "Gothic Classic", "game")]);
        assert_eq!(tie[0].score, 100);
        assert!(confident(&tie), "100 beats 92");
        let two = rank("Doom", vec![cand("1", "DOOM", "game"), cand("2", "Doom", "game")]);
        assert!(!confident(&two), "two equally good games: the user decides");
        assert!(!confident(&rank("Fallout", vec![cand("1", "Fallout 2", "game")])));
        assert!(!confident(&[]));
        // searched with other words: the game's own title still counts
        let r = rank_any(&["Baldurs Gate 3", "Baldur's Gate III"], vec![cand("1", "Baldur's Gate 3", "game")]);
        assert_eq!(r[0].score, 100);
        // another name of the game counts too (Databáze-her.cz lists translations)
        let mut zaklinac = cand("1", "The Witcher 3: Wild Hunt", "game");
        zaklinac.alt_titles = vec!["Zaklínač 3: Divoký hon".into()];
        assert_eq!(rank("Zaklínač 3 Divoký hon", vec![zaklinac])[0].score, 100);
    }

    #[test]
    fn descriptions_become_plain_text() {
        let html = "<p><b>Gather your party</b> &amp; return&nbsp;to the <i>Forgotten Realms</i>.</p>\n<img src=\"x.jpg\"><ul><li>Play solo</li><li>Co-op &#8211; 4 players</li></ul><hr><h4>About</h4>Line one<br/>Line two &copy; 2023<script>alert(1)</script>";
        assert_eq!(
            html_to_text(html),
            "Gather your party & return to the Forgotten Realms.\n\n\u{2022} Play solo\n\u{2022} Co-op \u{2013} 4 players\n\nAbout\n\nLine one\nLine two \u{a9} 2023"
        );
        assert_eq!(html_to_text("Tom &amp Jerry &unknown; 5 < 6"), "Tom &amp Jerry &unknown; 5 < 6");
        assert_eq!(html_to_text(""), "");
    }

    #[test]
    fn covers_get_small_previews() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
        assert_eq!(base64(&[0xff, 0xfe, 0x00]), "//4A");
        let img = image::RgbImage::from_pixel(342, 482, image::Rgb([200, 40, 40]));
        let mut png = std::io::Cursor::new(Vec::new());
        img.write_to(&mut png, image::ImageFormat::Png).unwrap();
        let p = cover_preview(&png.into_inner()).unwrap();
        assert!(p.starts_with("data:image/png;base64,iVBORw0KGgo"));
        assert!(cover_preview(b"<html>not a picture</html>").is_none());
    }

    #[test]
    fn names_are_joined_within_the_field_size() {
        let n = vec!["A".repeat(60), "B".repeat(30), "C".repeat(20)];
        assert_eq!(join_within(&n, 100), format!("{}, {}", "A".repeat(60), "B".repeat(30)));
        assert_eq!(join_within(&["X".repeat(150)], 100), "X".repeat(100));
        assert_eq!(join_within(&[], 10), "");
    }

    #[test]
    fn each_source_contacts_only_its_own_addresses() {
        for (s, ok) in [
            (Source::Gog, "https://catalog.gog.com/v1/catalog"),
            (Source::Gog, "https://images.gog-statics.com/a.jpg"),
            (Source::Dbher, "https://www.databaze-her.cz/hry/system-shock-2/"),
            (Source::Dbher, "https://databaze-her.cz/hry/x/"),
            (Source::Moby, "https://api.mobygames.com/v1/games?title=x"),
        ] {
            assert!(allowed_url(s, false, ok), "{ok}");
        }
        for (s, bad) in [
            (Source::Dbher, "http://www.databaze-her.cz/hry/x/"),
            (Source::Dbher, "https://evil.databaze-her.cz/x"),
            (Source::Dbher, "https://www.databaze-her.cz.evil.example/x"),
            (Source::Dbher, "https://api.gog.com/x"),
            (Source::Moby, "https://www.mobygames.com/game/1/"),
            (Source::Moby, "https://api.mobygames.com:8443/v1/games"),
            (Source::Gog, "https://api.mobygames.com/v1/games"),
            (Source::Moby, "http://127.0.0.1:9/x"),
        ] {
            assert!(!allowed_url(s, false, bad), "{bad}");
        }
        assert!(allowed_url(Source::Moby, true, "http://127.0.0.1:9/v1/games"));
        assert_eq!(Source::from_key("dbher"), Some(Source::Dbher));
        assert_eq!(Source::from_key("steam"), None);
    }

    #[test]
    fn the_mobygames_key_is_never_shown() {
        assert_eq!(redact("https://api.mobygames.com/v1/games?api_key=SECRET+1&title=x: timed out"), "https://api.mobygames.com/v1/games?api_key=\u{2026}&title=x: timed out");
        assert_eq!(redact("a api_key=x b api_key=y"), "a api_key=\u{2026} b api_key=\u{2026}");
        assert_eq!(redact("nothing here"), "nothing here");
        let e = LookupError::new(Source::Moby, NetError::Offline(redact("x?api_key=K")));
        assert!(!e.to_string().contains('K'));
    }

    #[test]
    fn descriptions_name_their_source() {
        let mut p = Candidate::new("system-shock-2", "System Shock 2");
        p.store_link = "https://www.databaze-her.cz/hry/system-shock-2/".into();
        let d = Details { source: Source::Dbher, product: p.clone(), description: Some("Hráč je odkázán sám na sebe.".into()), has_cover: false };
        assert_eq!(d.description(), "Hráč je odkázán sám na sebe.\n\n\u{2014} Zdroj: Databáze-her.cz, https://www.databaze-her.cz/hry/system-shock-2/ (licence CC BY-NC-ND 3.0 CZ)");
        let long = Details { description: Some("x".repeat(30_000)), ..d.clone() };
        assert_eq!(long.description().chars().count(), MAX_DESCRIPTION, "cut, but the source stays");
        assert!(long.description().ends_with("CC BY-NC-ND 3.0 CZ)"));
        let none = Details { description: Some("  ".into()), ..d.clone() };
        assert_eq!(none.description(), "", "no description, no source line");
        let gog = Details { source: Source::Gog, ..d };
        assert_eq!(gog.description(), "Hráč je odkázán sám na sebe.");
        p.store_link = "https://www.mobygames.com/game/1/".into();
        assert!(Details { source: Source::Moby, product: p, description: Some("A.".into()), has_cover: false }.description().ends_with("Source: MobyGames, https://www.mobygames.com/game/1/"));
    }
}

