fn main() {
    // Game Curator targets Windows only. Linux builds are the automated test
    // harness (`--features e2e`, with tauri.linux.conf.json for WebKitGTK).
    let windows = std::env::var("CARGO_CFG_WINDOWS").is_ok();
    let e2e = std::env::var("CARGO_FEATURE_E2E").is_ok();
    if !windows && !e2e {
        panic!("Game Curator is a Windows application; non-Windows builds need `--features e2e` (test harness).");
    }
    tauri_build::build()
}
