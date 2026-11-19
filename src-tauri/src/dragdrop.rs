//! Files dragged from Explorer onto the window.
//!
//! The paths stay on the Rust side: every dropped file gets a one-time token
//! (the same kind the file dialog gives), and the UI only learns the names,
//! sizes and where the pointer was. The panel under the pointer then asks to
//! add the files by their tokens (Screenshots, Additional files, a journal
//! entry, the gallery).

use std::path::PathBuf;

use serde::Serialize;
use tauri::{DragDropEvent, Emitter, Manager, WebviewWindow};

use crate::commands::{register_pick, PickedFile};
use crate::state::AppState;

#[derive(Serialize, Clone)]
struct DragPayload {
    kind: &'static str,
    x: f64,
    y: f64,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct DropPayload {
    pub x: f64,
    pub y: f64,
    pub files: Vec<PickedFile>,
    /// Folders and unreadable entries that were left out.
    pub skipped: usize,
}

pub fn handle(win: &WebviewWindow, ev: &DragDropEvent) {
    let scale = win.scale_factor().unwrap_or(1.0).max(0.1);
    let drag = |kind, x: f64, y: f64| {
        let _ = win.emit("gc-drag", DragPayload { kind, x: x / scale, y: y / scale });
    };
    match ev {
        DragDropEvent::Enter { position, .. } => drag("enter", position.x, position.y),
        DragDropEvent::Over { position } => drag("over", position.x, position.y),
        DragDropEvent::Drop { paths, position } => dropped(win, paths, position.x / scale, position.y / scale),
        DragDropEvent::Leave => drag("leave", 0.0, 0.0),
        _ => {}
    }
}

/// Registers the dropped files and tells the UI where they landed
/// (coordinates in CSS pixels).
pub fn dropped(win: &WebviewWindow, paths: &[PathBuf], x: f64, y: f64) {
    let state = win.state::<AppState>();
    let mut files = Vec::new();
    let mut skipped = 0;
    for p in paths.iter().take(200) {
        match register_pick(&state, p) {
            Some(f) => files.push(f),
            None => skipped += 1,
        }
    }
    skipped += paths.len().saturating_sub(200);
    let _ = win.emit("gc-drop", DropPayload { x, y, files, skipped });
}
