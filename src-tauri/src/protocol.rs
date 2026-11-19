//! `gcimg://` protocol: serves artwork and thumbnails from the archive to the
//! UI. Only `Data/Artwork/**`, `Data/Thumbnails/**` and `Data/Screenshots/**` are reachable.
//!
//! URL form (built by `convertFileSrc(path, 'gcimg')` in the frontend):
//!   Windows: `http://gcimg.localhost/<urlencoded relative path>`
//!   Linux:   `gcimg://localhost/<urlencoded relative path>`

use std::borrow::Cow;

use tauri::http::{Request, Response, StatusCode};
use tauri::{Manager, Runtime, UriSchemeContext};

use crate::state::AppState;

fn percent_decode(s: &str) -> Option<String> {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' if i + 2 < bytes.len() => {
                let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).ok()?;
                out.push(u8::from_str_radix(hex, 16).ok()?);
                i += 3;
            }
            b'%' => return None,
            b => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8(out).ok()
}

fn respond(status: StatusCode, mime: &str, body: Vec<u8>) -> Response<Cow<'static, [u8]>> {
    Response::builder()
        .status(status)
        .header("Content-Type", mime)
        .header("Cache-Control", "max-age=31536000, immutable")
        .header("Access-Control-Allow-Origin", "*")
        .body(Cow::<'static, [u8]>::Owned(body))
        .unwrap_or_else(|_| Response::new(Cow::Borrowed(&[][..])))
}

pub fn handle<R: Runtime>(ctx: UriSchemeContext<'_, R>, req: Request<Vec<u8>>) -> Response<Cow<'static, [u8]>> {
    let state = ctx.app_handle().state::<AppState>();
    let raw = req.uri().path().trim_start_matches('/');
    let Some(rel) = percent_decode(raw) else {
        return respond(StatusCode::BAD_REQUEST, "text/plain", b"bad path".to_vec());
    };
    let rel = rel.replace('\\', "/");
    match curator_core::images::read_archive_image(&state.layout, &rel) {
        Ok((bytes, mime)) => respond(StatusCode::OK, mime, bytes),
        Err(curator_core::CoreError::PathOutsideAllowed) => respond(StatusCode::FORBIDDEN, "text/plain", b"forbidden".to_vec()),
        Err(_) => respond(StatusCode::NOT_FOUND, "text/plain", b"not found".to_vec()),
    }
}

#[cfg(test)]
mod tests {
    use super::percent_decode;

    #[test]
    fn decoding() {
        assert_eq!(percent_decode("Data%2FThumbnails%2Fcover-1.webp").as_deref(), Some("Data/Thumbnails/cover-1.webp"));
        assert_eq!(percent_decode("a%20b").as_deref(), Some("a b"));
        assert_eq!(percent_decode("bad%2"), None);
        assert_eq!(percent_decode("bad%zz"), None);
    }
}
