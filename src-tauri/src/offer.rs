//! "Back from the game": the journal offer after playing.
//!
//! When a game started with Play has ended, a small window appears at once
//! in the middle of the screen, above all other windows - also when the
//! player has already switched to another program. It does not take the
//! keyboard away from the program in front. "Write entry" brings Game
//! Curator forward with a new journal entry for the game; "Not now" closes
//! the window; "Don't show this again" turns the offer off (Settings can
//! turn it back on). Two games ending one after the other are offered one
//! after the other.

use std::collections::VecDeque;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::session::GameEvent;
use crate::state::{err, lock, AppState};

pub const OFFER: &str = "journal-offer";

/// Games waiting for their offer (the first one is shown).
#[derive(Default)]
pub struct Offers {
    queue: VecDeque<GameEvent>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OfferInfo {
    game_id: i64,
    title: String,
    /// Further games waiting after this one.
    waiting: usize,
}

/// Offers a journal entry for a game that has just ended (unless the player
/// turned the offer off).
pub fn offer(app: &AppHandle, event: GameEvent) {
    let state = app.state::<AppState>();
    if !lock(&state.config).journal_prompt {
        return;
    }
    {
        let mut offers = lock(&state.offers);
        if offers.queue.iter().any(|e| e.game_id == event.game_id) {
            return;
        }
        offers.queue.push_back(event.clone());
    }
    if let Some(w) = app.get_webview_window(OFFER) {
        let _ = w.emit_to(OFFER, "gc-offer-changed", ());
        let _ = w.show();
        return;
    }
    let builder = crate::viewer::window_builder(app, OFFER, "offer.html", "Back from the game \u{2014} Game Curator")
        .inner_size(470.0, 262.0)
        .resizable(false)
        .maximizable(false)
        .minimizable(false)
        // in front of everything, but without taking the keyboard away
        .always_on_top(true)
        .focused(false)
        .center();
    match builder.build() {
        Ok(w) => {
            let handle = app.clone();
            w.on_window_event(move |ev| {
                if let tauri::WindowEvent::Destroyed = ev {
                    // closed with ✕: "not now" for every waiting game
                    lock(&handle.state::<AppState>().offers).queue.clear();
                }
            });
            let _ = w.request_user_attention(Some(tauri::UserAttentionType::Informational));
        }
        Err(e) => {
            // the window could not open: fall back to the dialog in the main window
            eprintln!("journal offer window: {e}");
            lock(&state.offers).queue.clear();
            let _ = app.emit_to("main", "gc-offer-fallback", &event);
        }
    }
}

/// The offer the window shows.
#[tauri::command]
pub async fn journal_offer(state: State<'_, AppState>) -> Result<Option<OfferInfo>, String> {
    let offers = lock(&state.offers);
    Ok(offers.queue.front().map(|e| OfferInfo { game_id: e.game_id, title: e.title.clone(), waiting: offers.queue.len() - 1 }))
}

/// The player's answer: write an entry now or not, and whether to offer
/// again after the next game.
#[tauri::command]
pub async fn answer_journal_offer(app: AppHandle, state: State<'_, AppState>, write: bool, never: bool) -> Result<(), String> {
    let current = lock(&state.offers).queue.pop_front();
    if never {
        let cfg = {
            let mut cfg = lock(&state.config).clone();
            cfg.journal_prompt = false;
            cfg
        };
        curator_core::config::save(&state.layout.config_path(), &cfg).map_err(err)?;
        lock(&state.config).journal_prompt = false;
        lock(&state.offers).queue.clear();
        let _ = app.emit_to("main", "gc-config-changed", ());
    }
    if let (true, Some(game)) = (write, current) {
        if let Some(main) = app.get_webview_window("main") {
            let _ = main.unminimize();
            let _ = main.show();
            let _ = main.set_focus();
            main.emit_to("main", "gc-journal-write", &game).map_err(|e| e.to_string())?;
        }
    }
    let more = !lock(&state.offers).queue.is_empty();
    if let Some(w) = app.get_webview_window(OFFER) {
        if more {
            let _ = w.emit_to(OFFER, "gc-offer-changed", ());
        } else {
            let _ = w.destroy();
        }
    }
    Ok(())
}
