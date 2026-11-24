//! Game details from GOG.com.
//!
//! Looks a game up in the public GOG store catalog by its title and offers
//! the official title, description, genres, tags, developer, publisher,
//! release year and (for games without one) the cover. What all sources
//! share - matching titles, the review, saving - is in `lookup.rs`.
//!
//! Only GOG addresses are ever contacted (HTTPS, `*.gog.com` and the image
//! servers `*.gog-statics.com`); the only thing sent is the game title being
//! looked up. No account, no cookies.

pub use crate::lookup::{
    all_games, apply, confident, core_title, cover_preview, current, games_missing_details, html_to_text, normalize, propose, rank, rank_any,
    score, Candidate, Change, Current, Details, LookupError, NetError, Source, FIELDS,
};
use crate::lookup::{str_list, year_of, Fetcher, MAX_IMAGE};
use crate::text;

// ---------------------------------------------------------------- addresses

/// Where the catalog and the product API are (tests use a local server).
#[derive(Debug, Clone)]
pub struct Endpoints {
    pub catalog: String,
    pub api: String,
    /// Tests only: plain HTTP to 127.0.0.1 / localhost is allowed too.
    pub allow_local_http: bool,
}

impl Endpoints {
    pub fn gog() -> Self {
        Endpoints { catalog: "https://catalog.gog.com".into(), api: "https://api.gog.com".into(), allow_local_http: false }
    }
}

/// Is this address one Game Curator may contact for GOG? HTTPS to GOG only
/// (plus the local test server when allowed).
pub fn allowed_url(ep: &Endpoints, raw: &str) -> bool {
    crate::lookup::allowed_url(Source::Gog, ep.allow_local_http, raw)
}

/// Errors of a GOG lookup (the shared lookup error, named by its source).
pub type GogError = LookupError;
type GogResult<T> = std::result::Result<T, LookupError>;

fn bad(e: String) -> LookupError {
    LookupError::new(Source::Gog, NetError::Bad(e))
}

// ---------------------------------------------------------------- catalog products

/// Reads the products of a catalog answer (`catalog.gog.com/v1/catalog`).
pub fn parse_catalog(json: &str) -> GogResult<Vec<Candidate>> {
    let v: serde_json::Value = serde_json::from_str(json).map_err(|e| bad(format!("catalog: {e}")))?;
    let products = v.get("products").and_then(|p| p.as_array()).ok_or_else(|| bad("catalog: no products".into()))?;
    let mut out = Vec::new();
    for p in products {
        let id = match p.get("id") {
            Some(serde_json::Value::String(s)) => s.clone(),
            Some(serde_json::Value::Number(n)) => n.to_string(),
            _ => continue,
        };
        let title = p.get("title").and_then(|t| t.as_str()).map(text::tidy).unwrap_or_default();
        if id.is_empty() || title.is_empty() || !id.chars().all(|c| c.is_ascii_digit()) {
            continue;
        }
        let cover = p.get("coverVertical").and_then(|c| c.as_str()).filter(|c| !c.is_empty()).map(str::to_string);
        out.push(Candidate {
            year: p.get("releaseDate").and_then(|d| d.as_str()).and_then(year_of),
            developers: str_list(p, "developers"),
            publishers: str_list(p, "publishers"),
            genres: str_list(p, "genres"),
            tags: str_list(p, "tags"),
            product_type: p.get("productType").and_then(|t| t.as_str()).unwrap_or("game").to_string(),
            store_link: p.get("storeLink").and_then(|t| t.as_str()).unwrap_or_default().to_string(),
            cover,
            ..Candidate::new(id, title)
        });
    }
    Ok(out)
}

// ---------------------------------------------------------------- the client

pub struct Client {
    f: Fetcher,
    ep: Endpoints,
}

impl Client {
    pub fn new(ep: Endpoints) -> Self {
        Client { f: Fetcher::new(Source::Gog, ep.allow_local_http), ep }
    }

    pub fn endpoints(&self) -> &Endpoints {
        &self.ep
    }

    fn get_json(&self, url: &str) -> GogResult<String> {
        self.f.get_text(url)
    }

    /// One catalog search.
    pub fn catalog(&self, query: &str) -> GogResult<Vec<Candidate>> {
        let url = url::Url::parse_with_params(
            &format!("{}/v1/catalog", self.ep.catalog.trim_end_matches('/')),
            &[
                ("limit", "12"),
                ("query", &format!("like:{query}")),
                ("productType", "in:game,pack"),
                ("locale", "en-US"),
                ("countryCode", "US"),
                ("currencyCode", "USD"),
            ],
        )
        .map_err(|e| bad(e.to_string()))?;
        parse_catalog(&self.get_json(url.as_str())?)
    }

    /// Looks a title up: the words of the title first, then - if that finds
    /// nothing good - the title without "The" and edition words.
    pub fn search(&self, title: &str) -> GogResult<Vec<Candidate>> {
        let q = normalize(title);
        if q.is_empty() {
            return Ok(Vec::new());
        }
        let mut found = rank(title, self.catalog(&q)?);
        let core = core_title(title);
        if !confident(&found) && core != q && !core.is_empty() {
            for c in self.catalog(&core)? {
                if !found.iter().any(|f| f.id == c.id) {
                    found.push(c);
                }
            }
            found = rank(title, found);
        }
        Ok(found)
    }

    /// The description of a product (plain text): the product page data
    /// first, then the older product API. `None` if GOG has none.
    pub fn description(&self, id: &str) -> GogResult<Option<String>> {
        if !id.chars().all(|c| c.is_ascii_digit()) || id.is_empty() {
            return Err(bad("product id".into()));
        }
        let api = self.ep.api.trim_end_matches('/');
        match self.get_json(&format!("{api}/v2/games/{id}?locale=en-US")) {
            Ok(json) => {
                if let Some(d) = serde_json::from_str::<serde_json::Value>(&json)
                    .ok()
                    .and_then(|v| v.get("description").and_then(|d| d.as_str()).map(html_to_text))
                    .filter(|d| !d.is_empty())
                {
                    return Ok(Some(d));
                }
            }
            Err(e) if matches!(e.kind, NetError::Offline(_)) => return Err(e),
            Err(_) => {}
        }
        match self.get_json(&format!("{api}/products/{id}?expand=description&locale=en-US")) {
            Ok(json) => {
                let v: serde_json::Value = serde_json::from_str(&json).map_err(|e| bad(e.to_string()))?;
                let d = v.get("description");
                let pick = |k: &str| d.and_then(|d| d.get(k)).and_then(|s| s.as_str()).map(html_to_text).filter(|s| !s.is_empty());
                Ok(pick("full").or_else(|| pick("lead")))
            }
            Err(e) if matches!(e.kind, NetError::NotFound | NetError::RateLimited) => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// The cover image of a product, if it has one.
    pub fn cover(&self, c: &Candidate) -> GogResult<Option<Vec<u8>>> {
        match &c.cover {
            Some(u) => {
                let u = if u.starts_with("//") { format!("https:{u}") } else { u.clone() };
                Ok(Some(self.f.get(&u, MAX_IMAGE)?))
            }
            None => Ok(None),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_gog_addresses_over_https() {
        let ep = Endpoints::gog();
        for ok in ["https://catalog.gog.com/v1/catalog?x=1", "https://api.gog.com/v2/games/1", "https://images.gog-statics.com/a.jpg", "https://www.gog.com/en/game/x"] {
            assert!(allowed_url(&ep, ok), "{ok}");
        }
        for bad in [
            "http://api.gog.com/v2/games/1",
            "https://gog.com.evil.example/x",
            "https://evilgog.com/x",
            "https://api.gog.com:8443/x",
            "https://user@api.gog.com/x",
            "ftp://api.gog.com/x",
            "http://127.0.0.1:8000/x",
            "file:///C:/x",
            "not a url",
        ] {
            assert!(!allowed_url(&ep, bad), "{bad}");
        }
        let test = Endpoints { allow_local_http: true, ..Endpoints::gog() };
        assert!(allowed_url(&test, "http://127.0.0.1:8000/v1/catalog"));
        assert!(!allowed_url(&test, "http://192.168.1.5/x"));
    }
}
