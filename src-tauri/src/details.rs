//! "Fetch details" on the Archive Scanner page: game details from GOG.com,
//! Databáze-her.cz or MobyGames (the user chooses the source).
//!
//! The lookup runs in a background thread (one game after another, gently)
//! and reports with `details-progress` / `details-finished`. Its results stay
//! here until the user has reviewed them: the page shows what the source
//! offers per game and saves only what the user ticks (`details_apply`). The
//! values saved come from the data held here, never from the page.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use curator_core::lookup::{self, Candidate, Change, LookupError, LookupResult, NetError, Source};
use curator_core::{dbher, gog, moby};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State};

use crate::state::{lock, AppState};

type CmdResult<T> = Result<T, String>;

/// One game's lookup.
struct Lookup {
    title: String,
    /// Products found, best first.
    candidates: Vec<Candidate>,
    /// The product whose details are offered.
    chosen: Option<String>,
    /// "found" (a sure match), "check" (please confirm), "not_found", "error"
    status: &'static str,
    error: Option<String>,
    /// GOG: per product, the description (None = GOG has none).
    descriptions: HashMap<String, Option<String>>,
    /// GOG: per product, the cover image and a small preview for the page.
    covers: HashMap<String, (Vec<u8>, String)>,
}

#[derive(Default)]
pub struct DetailsState {
    cancel: Option<Arc<AtomicBool>>,
    source: Source,
    games: Vec<(i64, Lookup)>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Progress {
    done: usize,
    total: usize,
    title: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Finished {
    source: Source,
    looked_up: usize,
    cancelled: bool,
    error: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LookupView {
    source: Source,
    game_id: i64,
    title: String,
    status: String,
    error: Option<String>,
    candidates: Vec<Candidate>,
    chosen: Option<String>,
    changes: Vec<Change>,
    cover_preview: Option<String>,
    store_link: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Counts {
    missing: usize,
    all: usize,
    /// The source of the results held now (if any).
    source: Source,
    /// MobyGames: is the user's key set?
    moby_key: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplyItem {
    game_id: i64,
    product_id: String,
    fields: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplyReport {
    saved: usize,
    saved_ids: Vec<i64>,
    errors: Vec<String>,
}

// ---------------------------------------------------------------- the sources

/// A client of the chosen source. The test build can point the sources at
/// a local server (GC_E2E_GOG_BASE, GC_E2E_DBHER_BASE, GC_E2E_MOBY_BASE).
enum Net {
    Gog(gog::Client),
    Dbher(dbher::Client),
    Moby(moby::Client),
}

#[allow(unused_mut)]
fn net(source: Source, moby_key: &str) -> LookupResult<Net> {
    let test_base = |_var: &str| -> Option<String> {
        #[cfg(feature = "e2e")]
        if let Ok(b) = std::env::var(_var) {
            return Some(b);
        }
        None
    };
    Ok(match source {
        Source::Gog => Net::Gog(gog::Client::new(match test_base("GC_E2E_GOG_BASE") {
            Some(b) => gog::Endpoints { catalog: b.clone(), api: b, allow_local_http: true },
            None => gog::Endpoints::gog(),
        })),
        Source::Dbher => Net::Dbher(dbher::Client::new(match test_base("GC_E2E_DBHER_BASE") {
            Some(b) => dbher::Endpoints { base: b, allow_local_http: true },
            None => dbher::Endpoints::site(),
        })),
        Source::Moby => Net::Moby(moby::Client::new(
            match test_base("GC_E2E_MOBY_BASE") {
                Some(b) => moby::Endpoints { api: b, allow_local_http: true },
                None => moby::Endpoints::api(),
            },
            moby_key,
        )?),
    })
}

impl Net {
    /// Looks a title up (or reads a page address of the source the user
    /// pasted), ranked against the game's own title too.
    fn find(&self, title: &str, query: &str) -> LookupResult<Vec<Candidate>> {
        match self {
            Net::Gog(c) => {
                let found = c.search(query)?;
                Ok(if query != title { lookup::rank_any(&[title, query], found) } else { found })
            }
            Net::Dbher(c) => c.lookup(query, title),
            Net::Moby(c) => c.lookup(query, title),
        }
    }

    /// What the chosen product still needs: GOG's description and cover,
    /// MobyGames' developers and publishers. Only "cannot go on" errors are
    /// passed on.
    fn complete(&self, look: &mut Lookup, product_id: &str, wants_cover: bool) -> LookupResult<()> {
        let Some(i) = look.candidates.iter().position(|c| c.id == product_id) else { return Ok(()) };
        match self {
            Net::Gog(client) => {
                let product = look.candidates[i].clone();
                if !look.descriptions.contains_key(&product.id) {
                    let d = match client.description(&product.id) {
                        Ok(d) => d,
                        Err(e) if e.stops_all() => return Err(e),
                        Err(_) => None,
                    };
                    look.descriptions.insert(product.id.clone(), d);
                }
                if wants_cover && product.cover.is_some() && !look.covers.contains_key(&product.id) {
                    match client.cover(&product) {
                        Ok(Some(bytes)) => {
                            if let Some(p) = lookup::cover_preview(&bytes) {
                                look.covers.insert(product.id.clone(), (bytes, p));
                            }
                        }
                        Ok(None) => {}
                        Err(e) if e.stops_all() => return Err(e),
                        Err(_) => {}
                    }
                }
            }
            Net::Dbher(_) => {}
            Net::Moby(client) => match client.companies(&mut look.candidates[i]) {
                Ok(()) => {}
                Err(e) if e.stops_all() => return Err(e),
                Err(_) => {}
            },
        }
        Ok(())
    }
}

/// Looks one title up. `Err` only when the source cannot be used at all
/// (no internet, too many requests, the MobyGames key).
fn look_up(net: &Net, title: &str, query: &str, has_artwork: bool) -> LookupResult<Lookup> {
    let mut look = Lookup {
        title: title.to_string(),
        candidates: Vec::new(),
        chosen: None,
        status: "not_found",
        error: None,
        descriptions: HashMap::new(),
        covers: HashMap::new(),
    };
    match net.find(title, query) {
        Ok(found) => {
            let sure = lookup::confident(&found);
            look.candidates = found;
            if let Some(best) = look.candidates.first().filter(|c| c.score >= 40).map(|c| c.id.clone()) {
                look.status = if sure { "found" } else { "check" };
                look.chosen = Some(best.clone());
                net.complete(&mut look, &best, !has_artwork)?;
            }
        }
        Err(e) if e.stops_all() => return Err(e),
        Err(e) => {
            look.status = "error";
            look.error = Some(e.to_string());
        }
    }
    Ok(look)
}

fn details_of(source: Source, look: &Lookup, product: &Candidate) -> lookup::Details {
    let cover = look.covers.get(&product.id);
    lookup::Details {
        source,
        description: look.descriptions.get(&product.id).cloned().flatten().or_else(|| product.description.clone()),
        has_cover: cover.is_some(),
        product: product.clone(),
    }
}

fn view(state: &AppState, source: Source, game_id: i64, look: &Lookup) -> Option<LookupView> {
    let cur = state.with_db(|c, _| lookup::current(c, game_id)).ok()?;
    let product = look.chosen.as_ref().and_then(|id| look.candidates.iter().find(|c| &c.id == id));
    let (changes, cover_preview, store_link) = match product {
        Some(p) => {
            let cover = look.covers.get(&p.id);
            (lookup::propose(&cur, &details_of(source, look, p)), cover.filter(|_| !cur.has_artwork).map(|c| c.1.clone()), p.store_link.clone())
        }
        None => (Vec::new(), None, String::new()),
    };
    Some(LookupView {
        source,
        game_id,
        title: cur.title,
        status: look.status.to_string(),
        error: look.error.clone(),
        candidates: look.candidates.clone(),
        chosen: look.chosen.clone(),
        changes,
        cover_preview,
        store_link,
    })
}

fn busy(state: &AppState) -> CmdResult<()> {
    if lock(&state.details).cancel.is_some() {
        return Err("Please wait until the lookup has finished.".to_string());
    }
    Ok(())
}

fn moby_key(state: &AppState) -> String {
    lock(&state.config).mobygames_key.clone()
}

fn parse_source(key: &str) -> CmdResult<Source> {
    Source::from_key(key).ok_or_else(|| "Unknown source of game details.".to_string())
}

// ---------------------------------------------------------------- commands

/// How many games the lookup would cover.
#[tauri::command]
pub async fn details_counts(state: State<'_, AppState>) -> CmdResult<Counts> {
    let (missing, all) = state.with_db(|c, _| Ok((lookup::games_missing_details(c)?.len(), lookup::all_games(c)?.len())))?;
    Ok(Counts { missing, all, source: lock(&state.details).source, moby_key: !moby_key(&state).is_empty() })
}

/// Starts looking games up at `source`: "missing" (games with details
/// missing), "all", or the given ids (games just imported).
#[tauri::command]
pub async fn details_start(app: AppHandle, state: State<'_, AppState>, source: String, scope: String, ids: Option<Vec<i64>>) -> CmdResult<usize> {
    let source = parse_source(&source)?;
    let wanted = match scope.as_str() {
        "missing" => state.with_db(|c, _| lookup::games_missing_details(c))?,
        "all" => state.with_db(|c, _| lookup::all_games(c))?,
        "ids" => ids.unwrap_or_default(),
        _ => return Err("Unknown choice of games.".to_string()),
    };
    if wanted.is_empty() {
        return Err("There are no games to look up.".to_string());
    }
    // a missing MobyGames key is reported before anything starts
    let client = net(source, &moby_key(&state)).map_err(|e| e.to_string())?;
    let cancel = {
        let mut g = lock(&state.details);
        if g.cancel.is_some() {
            return Err("A lookup is already running.".to_string());
        }
        let c = Arc::new(AtomicBool::new(false));
        g.cancel = Some(c.clone());
        g.source = source;
        g.games.clear();
        c
    };
    let total = wanted.len();
    std::thread::Builder::new()
        .name("details-lookup".into())
        .spawn(move || {
            let st = app.state::<AppState>();
            let mut error = None;
            let mut done = 0;
            for id in wanted {
                if cancel.load(Ordering::Relaxed) {
                    break;
                }
                let Ok(cur) = st.with_db(|c, _| lookup::current(c, id)) else { continue };
                let _ = app.emit("details-progress", Progress { done, total, title: cur.title.clone() });
                match look_up(&client, &cur.title, &cur.title, cur.has_artwork) {
                    Ok(look) => lock(&st.details).games.push((id, look)),
                    Err(e) => {
                        error = Some(e.to_string());
                        break;
                    }
                }
                done += 1;
            }
            let cancelled = cancel.load(Ordering::Relaxed);
            lock(&st.details).cancel = None;
            let _ = app.emit("details-finished", Finished { source, looked_up: done, cancelled, error });
        })
        .map_err(|e| {
            lock(&state.details).cancel = None;
            e.to_string()
        })?;
    Ok(total)
}

#[tauri::command]
pub async fn details_cancel(state: State<'_, AppState>) -> CmdResult<bool> {
    Ok(match lock(&state.details).cancel.as_ref() {
        Some(c) => {
            c.store(true, Ordering::Relaxed);
            true
        }
        None => false,
    })
}

/// The results to review.
#[tauri::command]
pub async fn details_results(state: State<'_, AppState>) -> CmdResult<Vec<LookupView>> {
    let g = lock(&state.details);
    Ok(g.games.iter().filter_map(|(id, look)| view(&state, g.source, *id, look)).collect())
}

/// Forgets the results (the review was closed).
#[tauri::command]
pub async fn details_clear(state: State<'_, AppState>) -> CmdResult<()> {
    busy(&state)?;
    lock(&state.details).games.clear();
    Ok(())
}

fn with_lookup<T>(app: &AppHandle, game_id: i64, f: impl FnOnce(Source, &mut Lookup) -> Result<T, String>) -> Result<T, String> {
    let st = app.state::<AppState>();
    let mut g = lock(&st.details);
    let source = g.source;
    let (_, look) = g.games.iter_mut().find(|(id, _)| *id == game_id).ok_or("This game is no longer in the results.")?;
    f(source, look)
}

/// Another of the products found is the right one.
#[tauri::command]
pub async fn details_choose(app: AppHandle, state: State<'_, AppState>, game_id: i64, product_id: String) -> CmdResult<LookupView> {
    busy(&state)?;
    let handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || -> CmdResult<LookupView> {
        let st = handle.state::<AppState>();
        let has_art = st.with_db(|c, _| lookup::current(c, game_id))?.has_artwork;
        // take the lookup out while the source is asked, so the lock is not held
        let (index, source, mut look) = {
            let mut g = lock(&st.details);
            let i = g.games.iter().position(|(id, _)| *id == game_id).ok_or("This game is no longer in the results.")?;
            (i, g.source, g.games.remove(i))
        };
        let result = (|| {
            if !look.1.candidates.iter().any(|c| c.id == product_id) {
                return Err("Unknown product.".to_string());
            }
            let client = net(source, &moby_key(&st)).map_err(|e| e.to_string())?;
            client.complete(&mut look.1, &product_id, !has_art).map_err(|e| e.to_string())?;
            look.1.chosen = Some(product_id);
            look.1.status = "found";
            Ok::<(), String>(())
        })();
        let v = view(&st, source, game_id, &look.1);
        {
            let mut g = lock(&st.details);
            let at = index.min(g.games.len());
            g.games.insert(at, look);
        }
        result?;
        v.ok_or_else(|| "The game no longer exists.".to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Searches again with words the user typed, or reads the page address of
/// the source the user pasted (a game the source did not find).
#[tauri::command]
pub async fn details_search(app: AppHandle, state: State<'_, AppState>, game_id: i64, query: String) -> CmdResult<LookupView> {
    busy(&state)?;
    let query = query.trim().to_string();
    if query.is_empty() {
        return Err("Please enter the title to look for.".to_string());
    }
    if query.chars().count() > 300 {
        return Err("The text to look for is too long.".to_string());
    }
    let handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || -> CmdResult<LookupView> {
        let st = handle.state::<AppState>();
        let source = lock(&st.details).source;
        let cur = st.with_db(|c, _| lookup::current(c, game_id))?;
        let client = net(source, &moby_key(&st)).map_err(|e| e.to_string())?;
        let look = look_up(&client, &cur.title, &query, cur.has_artwork).map_err(|e| e.to_string())?;
        let v = view(&st, source, game_id, &look);
        {
            let mut g = lock(&st.details);
            match g.games.iter_mut().find(|(id, _)| *id == game_id) {
                Some(slot) => slot.1 = look,
                None => g.games.push((game_id, look)),
            }
        }
        v.ok_or_else(|| "The game no longer exists.".to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Saves what the user ticked.
#[tauri::command]
pub async fn details_apply(app: AppHandle, state: State<'_, AppState>, items: Vec<ApplyItem>) -> CmdResult<ApplyReport> {
    busy(&state)?;
    let mut report = ApplyReport { saved: 0, saved_ids: Vec::new(), errors: Vec::new() };
    for item in items {
        if item.fields.is_empty() {
            continue;
        }
        // the details held here, for the product the user confirmed
        let prepared = with_lookup(&app, item.game_id, |source, look| {
            let product = look.candidates.iter().find(|c| c.id == item.product_id).cloned().ok_or("Unknown product.")?;
            let cover = look.covers.get(&product.id).map(|c| c.0.clone());
            Ok((details_of(source, look, &product), cover, look.title.clone()))
        });
        let (details, cover, title) = match prepared {
            Ok(p) => p,
            Err(e) => {
                report.errors.push(e);
                continue;
            }
        };
        match state.with_db(|c, l| lookup::apply(c, l, item.game_id, &details, &item.fields, cover.as_deref())) {
            Ok(_) => {
                report.saved += 1;
                report.saved_ids.push(item.game_id);
                lock(&state.details).games.retain(|(id, _)| *id != item.game_id);
            }
            Err(e) => report.errors.push(format!("{title}: {e}")),
        }
    }
    Ok(report)
}

// ---------------------------------------------------------------- the source in the browser

/// The source's own search for a title, in the user's browser (when the
/// title could not be found by Game Curator). The address is built here
/// from a fixed form; only the words are the user's.
#[tauri::command]
pub async fn details_open_search(source: String, query: String) -> CmdResult<()> {
    let source = parse_source(&source)?;
    let q: String = query.trim().chars().take(200).collect();
    let url = match source {
        Source::Gog => url::Url::parse_with_params("https://www.gog.com/en/games", &[("query", q.as_str())]),
        Source::Dbher => url::Url::parse_with_params("https://www.databaze-her.cz/hledani/", &[("fraze", q.as_str())]),
        Source::Moby => url::Url::parse_with_params("https://www.mobygames.com/search/", &[("q", q.as_str())]),
    }
    .map_err(|e| e.to_string())?;
    crate::system::open_url(url.as_str())
}

/// The page of the product chosen for a game, in the user's browser.
#[tauri::command]
pub async fn details_open_page(app: AppHandle, game_id: i64) -> CmdResult<()> {
    let (source, link) = with_lookup(&app, game_id, |source, look| {
        let p = look.chosen.as_ref().and_then(|id| look.candidates.iter().find(|c| &c.id == id)).ok_or("Unknown product.")?;
        Ok((source, p.store_link.clone()))
    })?;
    let ok = match url::Url::parse(&link) {
        Ok(u) if u.scheme() == "https" && u.port().is_none() => {
            let host = u.host_str().unwrap_or_default().to_ascii_lowercase();
            match source {
                Source::Gog => host == "www.gog.com" || host == "gog.com",
                Source::Dbher => host == "www.databaze-her.cz" || host == "databaze-her.cz",
                Source::Moby => host == "www.mobygames.com",
            }
        }
        _ => false,
    };
    if !ok {
        return Err(LookupError::new(source, NetError::NotAllowed(link)).to_string());
    }
    crate::system::open_url(&link)
}
