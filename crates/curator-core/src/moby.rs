//! English game details from MobyGames.
//!
//! Through the official MobyGames API (`api.mobygames.com/v1`) with the
//! user's own API key: MobyGames gives keys for non-commercial use from the
//! user's profile page, and the website itself is not meant to be read by
//! programs. The key is kept in `Data/config.json` next to Game Curator and
//! sent only to `api.mobygames.com` (as the API asks: in the address); it is
//! never shown in messages.
//!
//! The API allows at most one request a second and 360 an hour, so a game
//! takes one or two requests (the search, and the release of the chosen game
//! for its developer and publisher), a second apart.
//!
//! A game gives the title (and other names), an English description, genres
//! (the basic genres; perspective, setting, gameplay ... become tags),
//! developers, publishers and the year of the first release. Covers are not
//! taken.

use crate::lookup::{core_title, html_to_text, rank, rank_any, confident, year_of, Candidate, Fetcher, LookupResult, NetError, Source};
use crate::text;

/// Where the API is (tests use a local server).
#[derive(Debug, Clone)]
pub struct Endpoints {
    pub api: String,
    /// Tests only: plain HTTP to 127.0.0.1 / localhost is allowed too.
    pub allow_local_http: bool,
}

impl Endpoints {
    pub fn api() -> Self {
        Endpoints { api: "https://api.mobygames.com/v1".into(), allow_local_http: false }
    }
}

/// Longest API key accepted in Settings.
pub const MAX_KEY: usize = 200;

/// Is this a plausible API key (printable ASCII, no spaces)?
pub fn valid_key(key: &str) -> bool {
    key.len() <= MAX_KEY && key.chars().all(|c| c.is_ascii_graphic())
}

// ---------------------------------------------------------------- reading answers

/// Genre kinds that are not genres or tags worth keeping.
const SKIP_KINDS: &[&str] = &["business model", "interface", "add-on", "dlc", "misc", "educational", "special edition"];

/// Platforms whose releases name the companies best: Windows, DOS, Windows 3.x.
const PREFERRED_PLATFORMS: [i64; 3] = [3, 2, 5];

/// One game of an answer (`format=normal`).
pub fn parse_game(g: &serde_json::Value) -> Option<Candidate> {
    let id = g.get("game_id").and_then(|i| i.as_i64())?;
    let title = g.get("title").and_then(|t| t.as_str()).map(text::tidy).filter(|t| !t.is_empty())?;
    let mut c = Candidate::new(id.to_string(), crate::lookup::cut(&title, 200));
    c.store_link = g.get("moby_url").and_then(|u| u.as_str()).filter(|u| u.starts_with("https://www.mobygames.com/")).unwrap_or_default().to_string();
    if let Some(alts) = g.get("alternate_titles").and_then(|a| a.as_array()) {
        c.alt_titles = alts.iter().filter_map(|a| a.get("title").and_then(|t| t.as_str())).map(text::tidy).filter(|t| !t.is_empty()).take(12).collect();
    }
    for genre in g.get("genres").and_then(|a| a.as_array()).into_iter().flatten() {
        let name = genre.get("genre_name").and_then(|n| n.as_str()).map(text::tidy).unwrap_or_default();
        let kind = genre.get("genre_category").and_then(|n| n.as_str()).unwrap_or_default().to_lowercase();
        if name.is_empty() {
            continue;
        }
        if kind.contains("add-on") || name.to_lowercase().contains("dlc") {
            c.product_type = "dlc".into();
        }
        if kind == "basic genres" {
            if !c.genres.contains(&name) {
                c.genres.push(name);
            }
        } else if !SKIP_KINDS.iter().any(|s| kind.contains(s)) && !c.tags.contains(&name) {
            c.tags.push(name);
        }
    }
    let platforms: Vec<(i64, Option<i64>)> = g
        .get("platforms")
        .and_then(|a| a.as_array())
        .into_iter()
        .flatten()
        .filter_map(|p| Some((p.get("platform_id")?.as_i64()?, p.get("first_release_date").and_then(|d| d.as_str()).and_then(year_of))))
        .collect();
    c.year = platforms.iter().filter_map(|p| p.1).min();
    c.platform = PREFERRED_PLATFORMS.iter().find(|id| platforms.iter().any(|p| p.0 == **id)).copied().or_else(|| platforms.first().map(|p| p.0));
    c.description = g.get("description").and_then(|d| d.as_str()).map(html_to_text).filter(|d| !d.is_empty());
    Some(c)
}

/// The games of a search answer (`{"games": [...]}`).
pub fn parse_games(json: &str) -> Option<Vec<Candidate>> {
    let v: serde_json::Value = serde_json::from_str(json).ok()?;
    Some(v.get("games")?.as_array()?.iter().filter_map(parse_game).collect())
}

/// Developers and publishers of a release answer
/// (`/games/{id}/platforms/{platform}`).
pub fn parse_companies(json: &str) -> (Vec<String>, Vec<String>) {
    let mut dev = Vec::new();
    let mut publ = Vec::new();
    let Ok(v) = serde_json::from_str::<serde_json::Value>(json) else { return (dev, publ) };
    for release in v.get("releases").and_then(|r| r.as_array()).into_iter().flatten() {
        for co in release.get("companies").and_then(|c| c.as_array()).into_iter().flatten() {
            let name = co.get("company_name").and_then(|n| n.as_str()).map(text::tidy).unwrap_or_default();
            let role = co.get("role").and_then(|r| r.as_str()).unwrap_or_default().to_lowercase();
            if name.is_empty() {
                continue;
            }
            let list = if role.starts_with("developed") {
                &mut dev
            } else if role.starts_with("published") {
                &mut publ
            } else {
                continue;
            };
            if !list.contains(&name) {
                list.push(name);
            }
        }
    }
    (dev, publ)
}

/// The game id in a MobyGames address the user pasted
/// (`https://www.mobygames.com/game/1234/system-shock-2/`), if it is one.
pub fn id_from_input(input: &str) -> Option<String> {
    let s = input.trim().to_ascii_lowercase();
    if !s.contains("mobygames.com") {
        return None;
    }
    let after = &s[s.find("/game/")? + 6..];
    let id: String = after.chars().take_while(char::is_ascii_digit).collect();
    (!id.is_empty() && id.len() <= 9).then_some(id)
}

// ---------------------------------------------------------------- the client

pub struct Client {
    f: Fetcher,
    ep: Endpoints,
    key: String,
}

impl Client {
    /// Needs the user's API key.
    pub fn new(ep: Endpoints, key: &str) -> LookupResult<Self> {
        let f = Fetcher::new(Source::Moby, ep.allow_local_http);
        let key = key.trim();
        if key.is_empty() {
            return Err(f.err(NetError::NoKey));
        }
        if !valid_key(key) {
            return Err(f.err(NetError::Unauthorized));
        }
        Ok(Client { f, ep, key: key.to_string() })
    }

    fn url(&self, path: &str, params: &[(&str, &str)]) -> LookupResult<String> {
        let mut all = vec![("api_key", self.key.as_str())];
        all.extend_from_slice(params);
        url::Url::parse_with_params(&format!("{}{path}", self.ep.api.trim_end_matches('/')), &all)
            .map(|u| u.to_string())
            .map_err(|e| self.f.err(NetError::Bad(e.to_string())))
    }

    fn get_json(&self, path: &str, params: &[(&str, &str)]) -> LookupResult<String> {
        self.f.get_text(&self.url(path, params)?)
    }

    /// Games whose title contains the words.
    pub fn find(&self, title: &str) -> LookupResult<Vec<Candidate>> {
        let title: String = title.chars().take(128).collect();
        let json = self.get_json("/games", &[("title", &title), ("format", "normal"), ("limit", "10")])?;
        parse_games(&json).ok_or_else(|| self.f.err(NetError::Bad("games".into())))
    }

    /// Looks a title up: as written first, then - if that finds nothing
    /// sure - without "The" and the edition words.
    pub fn search(&self, title: &str) -> LookupResult<Vec<Candidate>> {
        let t = text::tidy(title);
        if t.is_empty() {
            return Ok(Vec::new());
        }
        let mut found = rank(&t, self.find(&t)?);
        let core = core_title(&t);
        if !confident(&found) && !core.is_empty() && core != t.to_lowercase() {
            for c in self.find(&core)? {
                if !found.iter().any(|f| f.id == c.id) {
                    found.push(c);
                }
            }
            found = rank(&t, found);
        }
        Ok(found)
    }

    /// One game by its id; `None` if MobyGames has no such game.
    pub fn game(&self, id: &str) -> LookupResult<Option<Candidate>> {
        if id.is_empty() || !id.chars().all(|c| c.is_ascii_digit()) {
            return Err(self.f.err(NetError::Bad("game id".into())));
        }
        match self.get_json(&format!("/games/{id}"), &[("format", "normal")]) {
            Ok(json) => Ok(serde_json::from_str::<serde_json::Value>(&json).ok().as_ref().and_then(parse_game)),
            Err(e) if e.kind == NetError::NotFound => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// What the user typed: a MobyGames address is read as it is, anything
    /// else is a title to look for.
    pub fn lookup(&self, input: &str, title: &str) -> LookupResult<Vec<Candidate>> {
        match id_from_input(input) {
            Some(id) => Ok(self.game(&id)?.map(|c| rank_any(&[title, &c.title.clone()], vec![c])).unwrap_or_default()),
            None => self.search(input).map(|found| rank_any(&[title, input], found)),
        }
    }

    /// Fills in the developers and publishers of a game (from its release
    /// on Windows or DOS when it has one).
    pub fn companies(&self, c: &mut Candidate) -> LookupResult<()> {
        let Some(platform) = c.platform else { return Ok(()) };
        if !c.id.chars().all(|ch| ch.is_ascii_digit()) {
            return Err(self.f.err(NetError::Bad("game id".into())));
        }
        match self.get_json(&format!("/games/{}/platforms/{platform}", c.id), &[]) {
            Ok(json) => {
                let (dev, publ) = parse_companies(&json);
                c.developers = dev;
                c.publishers = publ;
                c.platform = None;
                Ok(())
            }
            Err(e) if e.kind == NetError::NotFound => {
                c.platform = None;
                Ok(())
            }
            Err(e) => Err(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The shape of a real answer (api.mobygames.com/v1/games?format=normal), shortened.
    pub const GAMES: &str = r#"{"games":[
      {"game_id":491,"title":"System Shock 2","moby_url":"https://www.mobygames.com/game/491/system-shock-2/","moby_score":8.9,
       "alternate_titles":[{"description":"Working title","title":"System Shock: The Next Generation"}],
       "description":"<p>The year is 2114. You wake up aboard the <i>Von Braun</i>&hellip;</p><p>Choose a career.</p>",
       "genres":[{"genre_category":"Basic Genres","genre_category_id":1,"genre_id":1,"genre_name":"Action"},
                 {"genre_category":"Basic Genres","genre_category_id":1,"genre_id":50,"genre_name":"Role-playing (RPG)"},
                 {"genre_category":"Perspective","genre_category_id":2,"genre_id":7,"genre_name":"1st-person"},
                 {"genre_category":"Setting","genre_category_id":10,"genre_id":3,"genre_name":"Sci-fi / futuristic"},
                 {"genre_category":"Business Model","genre_category_id":13,"genre_id":161,"genre_name":"Commercial"}],
       "platforms":[{"first_release_date":"2023-06","platform_id":141,"platform_name":"Xbox One"},
                    {"first_release_date":"1999-08","platform_id":3,"platform_name":"Windows"}],
       "sample_cover":{"image":"https://cdn.mobygames.com/covers/1.jpg"}},
      {"game_id":3307,"title":"System Shock","moby_url":"https://www.mobygames.com/game/3307/system-shock/","description":null,
       "genres":[],"platforms":[{"first_release_date":"1994","platform_id":2,"platform_name":"DOS"}]},
      {"title":"broken entry without id"}
    ]}"#;

    #[test]
    fn games_are_read() {
        let g = parse_games(GAMES).unwrap();
        assert_eq!(g.len(), 2, "the broken entry is left out");
        let ss2 = &g[0];
        assert_eq!((ss2.id.as_str(), ss2.title.as_str()), ("491", "System Shock 2"));
        assert_eq!(ss2.alt_titles, ["System Shock: The Next Generation"]);
        assert_eq!(ss2.genres, ["Action", "Role-playing (RPG)"]);
        assert_eq!(ss2.tags, ["1st-person", "Sci-fi / futuristic"], "business model is no tag");
        assert_eq!(ss2.year, Some(1999), "the first release");
        assert_eq!(ss2.platform, Some(3), "Windows names the companies");
        assert_eq!(ss2.store_link, "https://www.mobygames.com/game/491/system-shock-2/");
        assert_eq!(ss2.description.as_deref(), Some("The year is 2114. You wake up aboard the Von Braun\u{2026}\n\nChoose a career."));
        assert_eq!(ss2.cover, None, "covers are not taken from MobyGames");
        assert_eq!((g[1].description.clone(), g[1].platform, g[1].year), (None, Some(2), Some(1994)));
        assert!(parse_games("{}").is_none());
        assert!(parse_games("not json").is_none());
    }

    #[test]
    fn companies_and_addresses() {
        let rel = r#"{"game_id":491,"platform_id":3,"releases":[
          {"companies":[{"company_id":1,"company_name":"Irrational Games","role":"Developed by"},
                        {"company_id":2,"company_name":"Looking Glass Studios","role":"Developed by"},
                        {"company_id":3,"company_name":"Electronic Arts","role":"Published by"},
                        {"company_id":4,"company_name":"Somebody","role":"Ported by"}],"release_date":"1999-08-11"},
          {"companies":[{"company_id":3,"company_name":"Electronic Arts","role":"Published by"}],"release_date":"1999"}]}"#;
        assert_eq!(
            parse_companies(rel),
            (vec!["Irrational Games".to_string(), "Looking Glass Studios".to_string()], vec!["Electronic Arts".to_string()])
        );
        assert_eq!(parse_companies("{}"), (vec![], vec![]));
        assert_eq!(id_from_input("https://www.mobygames.com/game/491/system-shock-2/").as_deref(), Some("491"));
        assert_eq!(id_from_input("mobygames.com/game/3307").as_deref(), Some("3307"));
        assert_eq!(id_from_input("System Shock 2"), None);
        assert_eq!(id_from_input("https://www.mobygames.com/company/1/"), None);
        assert!(valid_key("moby_AbC.123-xyz") && !valid_key("with space") && !valid_key(&"x".repeat(201)));
        let no = Client::new(Endpoints::api(), "  ").err().unwrap();
        assert_eq!(no.kind, NetError::NoKey);
        assert!(no.to_string().contains("Settings"));
    }
}
