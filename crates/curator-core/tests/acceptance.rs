//! Core-level versions of the acceptance tests from the specification
//! (section 14). UI-level checks are done by the end-to-end tests.

mod common;

use std::fs;
use std::sync::atomic::AtomicBool;
use std::time::Instant;

use common::*;
use curator_core::archive::{self, ArchiveIdStatus, OpenedArchive};
use curator_core::games::{self, ArtworkChange, GameInput, GamePatch, LibraryQuery};
use curator_core::rusqlite;
use curator_core::journal::{self, JournalInput, JournalQuery};
use curator_core::scanner::{self, ImportItem};
use curator_core::images::Crop;
use curator_core::installs::{self, InstalledProgram};
use curator_core::additions::{self, AdditionInput};
use curator_core::screenshots;
use curator_core::{backup, files, images, paths, CoreError};

// ------------------------------------------------------------- Test 1 + 2

#[test]
fn t1_t2_archive_survives_move_and_drive_letter_change() {
    let fx = Fixture::new();
    let mut a = fx.open("D");
    assert_eq!(a.id_status, ArchiveIdStatus::Created);
    assert!(a.layout.id_file().is_file(), ".gamecurator-id must be created on first run");
    let archive_id = a.archive_id.clone();

    // game inside Games/ and one in an external folder elsewhere on the drive
    let inside = a.layout.games_dir().join("System Shock 2");
    make_game_folder(&inside, "system_shock_2");
    let external = fx.drive("D").join("GOG_Installers").join("Game_A");
    make_game_folder(&external, "game_a");
    let id1 = add_game(&mut a, "System Shock 2", &inside);
    let id2 = add_game(&mut a, "Game A", &external);

    // artwork + journal
    let png = fx.tmp.path().join("cover.png");
    write_png(&png, 600, 900);
    let art = images::process_artwork(&a.layout, &png).unwrap();
    let layout = a.layout.clone();
    let mut inp = input("System Shock 2", &inside);
    inp.artwork = Some(ArtworkChange::Set { artwork_path: art.artwork_path.clone(), thumbnail_path: art.thumbnail_path.clone(), crop: None });
    games::update_game(a.db.conn().unwrap(), &layout, id1, &inp).unwrap();
    let entry = journal::create_entry(
        a.db.conn().unwrap(),
        &JournalInput { game_id: id1, entry_date: "2026-11-18".into(), title: "Von Braun".into(), content: "<p>Spooky.</p>".into() },
    )
    .unwrap();

    // stored paths are relative to the archive root
    let g1 = games::get_game(a.db.conn().unwrap(), &layout, id1).unwrap();
    let g2 = games::get_game(a.db.conn().unwrap(), &layout, id2).unwrap();
    assert_eq!(g1.folder_path, "Games/System Shock 2");
    assert_eq!(g2.folder_path, "../GOG_Installers/Game_A");
    assert!(g1.folder_portable && g2.folder_portable);
    a.db.close();
    drop(a);

    // "Move the whole archive to another computer / change the drive letter"
    fs::rename(fx.drive("D"), fx.drive("E")).unwrap();
    let mut b = fx.open("E");
    assert_eq!(b.id_status, ArchiveIdStatus::Ok);
    assert_eq!(b.archive_id, archive_id);
    let layout = b.layout.clone();
    let g1 = games::get_game(b.db.conn().unwrap(), &layout, id1).unwrap();
    let g2 = games::get_game(b.db.conn().unwrap(), &layout, id2).unwrap();
    assert!(g1.folder_exists, "inside game folder resolves after move");
    assert!(g2.folder_exists, "external game folder resolves after move");
    assert!(g1.folder_resolved.contains("/E/") || g1.folder_resolved.contains("\\E\\"));
    assert!(layout.root.join(g1.artwork_path.as_ref().unwrap()).is_file(), "artwork still available");
    assert!(layout.root.join(g1.thumbnail_path.as_ref().unwrap()).is_file(), "thumbnail still available");
    let e = journal::get_entry(b.db.conn().unwrap(), entry.id).unwrap();
    assert_eq!(e.content, "<p>Spooky.</p>");
    let report = games::check_availability(b.db.conn().unwrap(), &layout.root).unwrap();
    assert_eq!(report.unavailable, 0);
}

#[test]
fn archive_id_file_is_restored_and_mismatch_detected() {
    let fx = Fixture::new();
    let a = fx.open("D");
    let id = a.archive_id.clone();
    let id_file = a.layout.id_file();
    drop(a);

    fs::remove_file(&id_file).unwrap();
    let b = fx.open("D");
    assert_eq!(b.id_status, ArchiveIdStatus::IdFileRestored);
    assert_eq!(archive::read_id_file(&id_file).as_deref(), Some(id.as_str()));
    drop(b);

    let other = "8d0c1f9e-1111-4222-8333-944445555666";
    archive::write_id_file(&id_file, other).unwrap();
    let mut c = fx.open("D");
    assert!(matches!(c.id_status, ArchiveIdStatus::Mismatch { .. }));
    let adopted = archive::adopt_database(&c.layout.clone(), &mut c.db).unwrap();
    assert_eq!(adopted, other);
    drop(c);
    assert_eq!(fx.open("D").id_status, ArchiveIdStatus::Ok);
}

// ------------------------------------------------------------- Test 3

#[test]
fn t3_game_management_never_touches_the_folder() {
    let fx = Fixture::new();
    let mut a = fx.open("D");
    let layout = a.layout.clone();
    let folder = layout.games_dir().join("Disco Elysium");
    make_game_folder(&folder, "disco_elysium");

    let mut inp = input("Disco Elysium", &folder);
    inp.tags = vec!["RPG, detective".into(), "rpg".into()];
    inp.genre = "RPG".into();
    let id = games::create_game(a.db.conn().unwrap(), &layout, &inp).unwrap();
    let g = games::get_game(a.db.conn().unwrap(), &layout, id).unwrap();
    assert_eq!(g.tags, vec!["detective", "RPG"], "tags are split, trimmed and de-duplicated");

    // artwork upload
    let png = fx.tmp.path().join("de.jpg");
    image::RgbImage::from_pixel(1000, 1500, image::Rgb([200, 40, 40])).save(&png).unwrap();
    let art = images::process_artwork(&layout, &png).unwrap();
    inp.artwork = Some(ArtworkChange::Set { artwork_path: art.artwork_path.clone(), thumbnail_path: art.thumbnail_path.clone(), crop: None });
    inp.description = "A detective RPG.".into();
    inp.tags.push("favourite-2019".into());
    games::update_game(a.db.conn().unwrap(), &layout, id, &inp).unwrap();
    let g = games::get_game(a.db.conn().unwrap(), &layout, id).unwrap();
    assert_eq!(g.description, "A detective RPG.");
    assert!(g.tags.contains(&"favourite-2019".to_string()));
    assert!(layout.root.join(&art.artwork_path).is_file());

    journal::create_entry(
        a.db.conn().unwrap(),
        &JournalInput { game_id: id, entry_date: "2026-11-01".into(), title: "Day 1".into(), content: "<p>x</p>".into() },
    )
    .unwrap();
    let preview = games::removal_preview(a.db.conn().unwrap(), &layout, id).unwrap();
    assert_eq!(preview.journal_entries, 1);

    let files_before: Vec<_> = files::list_folder(&folder, 5, 1000).unwrap().files.into_iter().map(|f| f.rel_path).collect();
    let summary = games::remove_game(a.db.conn().unwrap(), &layout, id).unwrap();
    assert_eq!(summary.journal_entries_removed, 1);
    let files_after: Vec<_> = files::list_folder(&folder, 5, 1000).unwrap().files.into_iter().map(|f| f.rel_path).collect();
    assert_eq!(files_before, files_after, "the original folder and files must stay untouched");
    assert!(matches!(games::get_game(a.db.conn().unwrap(), &layout, id), Err(CoreError::NotFound(_))));
    assert!(!layout.root.join(&art.artwork_path).exists(), "our own artwork copy is cleaned up");
    assert!(games::library_facets(a.db.conn().unwrap()).unwrap().tags.is_empty(), "orphan tags removed");
}

#[test]
fn duplicates_and_validation() {
    let fx = Fixture::new();
    let mut a = fx.open("D");
    let layout = a.layout.clone();
    let folder = layout.games_dir().join("Fallout 2");
    make_game_folder(&folder, "fallout_2");
    add_game(&mut a, "Fallout 2", &folder);

    let dup = games::create_game(a.db.conn().unwrap(), &layout, &input("Fallout 2 again", &folder.join(".")));
    assert!(matches!(dup, Err(CoreError::Invalid(ref m)) if m.contains("already assigned")), "{dup:?}");

    let no_title = games::create_game(a.db.conn().unwrap(), &layout, &input("  ", &folder));
    assert!(matches!(no_title, Err(CoreError::Invalid(_))));
    let missing = games::create_game(a.db.conn().unwrap(), &layout, &input("X", &layout.games_dir().join("nope")));
    assert!(matches!(missing, Err(CoreError::Invalid(ref m)) if m.contains("does not exist")));
    let data = games::create_game(a.db.conn().unwrap(), &layout, &input("X", &layout.data_dir()));
    assert!(matches!(data, Err(CoreError::Invalid(ref m)) if m.contains("Game Curator itself")));
    let mut bad_year = input("Y", &layout.games_dir());
    bad_year.release_year = Some(1800);
    assert!(games::create_game(a.db.conn().unwrap(), &layout, &bad_year).is_err());
}

#[test]
fn library_search_sort_facets_and_file_stats() {
    let fx = Fixture::new();
    let mut a = fx.open("D");
    let layout = a.layout.clone();
    let mk = |a: &mut curator_core::archive::OpenedArchive, title: &str, genre: &str, tags: &[&str]| {
        let folder = layout.games_dir().join(title);
        fs::create_dir_all(&folder).unwrap();
        let inp = GameInput {
            title: title.into(),
            folder_path: folder.to_string_lossy().into(),
            genre: genre.into(),
            tags: tags.iter().map(|s| s.to_string()).collect(),
            ..Default::default()
        };
        games::create_game(a.db.conn().unwrap(), &layout, &inp).unwrap()
    };
    mk(&mut a, "Zaklínač 3", "RPG", &["open world"]);
    let mafia = mk(&mut a, "Mafia", "Action", &["czech"]);
    mk(&mut a, "Vietcong", "Shooter", &["czech"]);
    mk(&mut a, "Baldur's Gate 3", "rpg", &[]);
    let q = |search: Option<&str>, sort: Option<&str>| LibraryQuery { search: search.map(Into::into), sort: sort.map(Into::into) };
    let titles = |v: Vec<games::GameCard>| v.into_iter().map(|g| g.title).collect::<Vec<_>>();
    {
        let c = a.db.conn().unwrap();
        assert_eq!(titles(games::list_games(c, &q(Some("zaklinac"), None), "TEST-PC").unwrap()), vec!["Zaklínač 3"]);
        assert_eq!(titles(games::list_games(c, &q(Some("GATE"), None), "TEST-PC").unwrap()), vec!["Baldur's Gate 3"]);
        assert_eq!(games::list_games(c, &q(Some("%"), None), "TEST-PC").unwrap().len(), 0, "wildcards are escaped");
        assert_eq!(titles(games::list_games(c, &q(None, None), "TEST-PC").unwrap()), vec!["Baldur's Gate 3", "Mafia", "Vietcong", "Zaklínač 3"]);
        assert_eq!(games::list_games(c, &q(None, Some("added")), "TEST-PC").unwrap()[0].title, "Baldur's Gate 3");
        // the whole archive is always listed: unknown/old filter fields are simply ignored
        let legacy: LibraryQuery = serde_json::from_str(r#"{"filter":"completed","tag":"czech","genre":"RPG"}"#).unwrap();
        assert_eq!(games::list_games(c, &legacy, "TEST-PC").unwrap().len(), 4);
        let f = games::library_facets(c).unwrap();
        assert_eq!((f.total, f.unavailable), (4, 0));
        assert_eq!(f.genres.iter().find(|g| g.name.eq_ignore_ascii_case("rpg")).unwrap().count, 2, "genre suggestions for the form");
        assert_eq!(f.tags.iter().find(|t| t.name == "czech").unwrap().count, 2, "tag suggestions for the form");
    }

    // file statistics per game (missing files are not counted), size sort
    let folder = layout.games_dir().join("Mafia");
    touch(&folder.join("setup_mafia.exe"), 3000);
    touch(&folder.join("Manual.pdf"), 1000);
    files::archive_content(a.db.conn().unwrap(), &layout, mafia).unwrap();
    fs::remove_file(folder.join("Manual.pdf")).unwrap();
    files::archive_content(a.db.conn().unwrap(), &layout, mafia).unwrap();
    let c = a.db.conn().unwrap();
    let by_size = games::list_games(c, &LibraryQuery { sort: Some("size".into()), ..Default::default() }, "TEST-PC").unwrap();
    assert_eq!(by_size[0].title, "Mafia");
    assert_eq!((by_size[0].file_count, by_size[0].total_size), (1, 3000));
    assert_eq!(by_size[0].folder_path, "Games/Mafia");

    games::patch_game(c, mafia, &GamePatch { archive_notes: Some("Czech release, 2 CDs, v1.2 patch.".into()) }).unwrap();
    assert_eq!(games::get_game(c, &layout, mafia).unwrap().archive_notes, "Czech release, 2 CDs, v1.2 patch.");
}

#[test]
fn database_v1_is_upgraded_with_a_backup_and_no_data_loss() {
    let fx = Fixture::new();
    let root = fx.archive_root("D");
    let data = root.join("Data");
    fs::create_dir_all(&data).unwrap();
    let game_dir = root.join("Games").join("Old Game");
    fs::create_dir_all(&game_dir).unwrap();
    {
        // a database written by Game Curator 0.1 (schema v1, with play status etc.)
        let c = rusqlite::Connection::open(data.join("library.db")).unwrap();
        c.execute_batch(include_str!("fixtures/schema_v1.sql")).unwrap();
        c.execute_batch(
            "PRAGMA user_version = 1;
             INSERT INTO games (title, title_search, folder_path, folder_key, status, rating, is_favorite, archive_notes)
                VALUES ('Old Game', 'old game', 'Games/Old Game', 'games/old game', 'completed', 5, 1, 'boxed edition');
             INSERT INTO journal_entries (game_id, entry_date, title, content, rating) VALUES (1, '2026-01-02', 'Memories', '<p>hi</p>', 4);
             INSERT INTO tags (name) VALUES ('boxed'); INSERT INTO game_tags VALUES (1, 1);
             INSERT INTO settings (setting_key, setting_value) VALUES ('archive_id', '8d0c1f9e-1111-4222-8333-944445555666');",
        )
        .unwrap();
    }
    archive::write_id_file(&root.join(".gamecurator-id"), "8d0c1f9e-1111-4222-8333-944445555666").unwrap();

    let mut a = fx.open("D");
    assert_eq!(a.id_status, ArchiveIdStatus::Ok);
    let layout = a.layout.clone();
    let c = a.db.conn().unwrap();
    let version: i64 = c.query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap();
    assert_eq!(version, curator_core::db::SCHEMA_VERSION);
    let cols: Vec<String> = c
        .prepare("SELECT name FROM pragma_table_info('games')").unwrap()
        .query_map([], |r| r.get(0)).unwrap().collect::<Result<_, _>>().unwrap();
    for gone in ["status", "rating", "is_favorite"] {
        assert!(!cols.contains(&gone.to_string()), "{gone} column removed");
    }
    assert!(cols.contains(&"cover_crop".to_string()), "v3: cover crop column");
    let installs: i64 = c.query_row("SELECT COUNT(*) FROM installations", [], |r| r.get(0)).unwrap();
    assert_eq!(installs, 0, "v3: installations table");
    let v4: i64 = c.query_row("SELECT (SELECT COUNT(*) FROM screenshots) + (SELECT COUNT(*) FROM additions)", [], |r| r.get(0)).unwrap();
    assert_eq!(v4, 0, "v4: screenshots and additions tables");
    let g = games::get_game(c, &layout, 1).unwrap();
    assert_eq!((g.title.as_str(), g.archive_notes.as_str(), g.tags.clone()), ("Old Game", "boxed edition", vec!["boxed".to_string()]));
    assert_eq!(journal::get_entry(c, 1).unwrap().title, "Memories");
    let integrity: String = c.query_row("PRAGMA integrity_check", [], |r| r.get(0)).unwrap();
    assert_eq!(integrity, "ok");
    let backups = backup::list_backups(&layout).unwrap();
    let pre = backups.iter().find(|b| b.kind == "pre-upgrade").expect("pre-upgrade backup written");
    let check = backup::verify(&layout.backups_dir().join(&pre.file_name)).unwrap();
    assert_eq!(check.games, 1, "the backup holds the original v1 data");
    // opening again does not create another backup
    drop(a);
    let b = fx.open("D");
    assert_eq!(backup::list_backups(&b.layout).unwrap().iter().filter(|x| x.kind == "pre-upgrade").count(), 1);
}

// ------------------------------------------------------------- Test 4

#[test]
fn t4_archive_content_categories_primary_and_safety() {
    let fx = Fixture::new();
    let mut a = fx.open("D");
    let layout = a.layout.clone();
    let folder = layout.games_dir().join("Gothic");
    make_game_folder(&folder, "gothic");
    touch(&folder.join("GothicStarter.exe"), 10);
    let id = add_game(&mut a, "Gothic", &folder);

    let content = files::archive_content(a.db.conn().unwrap(), &layout, id).unwrap();
    assert!(content.folder_available && content.refreshed);
    let by_name = |n: &str| content.files.iter().find(|f| f.filename == n).unwrap().clone();
    assert_eq!(by_name("setup_gothic_1.0.2.exe").category, "installer");
    assert_eq!(by_name("GothicStarter.exe").category, "executable", "not every exe is an installer");
    assert_eq!(by_name("Manual.pdf").category, "manual");
    assert_eq!(by_name("01 - Theme.mp3").file_path, "Soundtrack/01 - Theme.mp3");
    assert!(content.dirs.iter().any(|d| d.rel_path == "Soundtrack"));

    // custom category + primary installer
    let starter = by_name("GothicStarter.exe");
    files::set_category(a.db.conn().unwrap(), id, starter.id, Some("installer")).unwrap();
    files::set_primary(a.db.conn().unwrap(), id, Some(starter.id)).unwrap();
    let setup = by_name("setup_gothic_1.0.2.exe");
    files::set_primary(a.db.conn().unwrap(), id, Some(setup.id)).unwrap();
    let content = files::archive_content(a.db.conn().unwrap(), &layout, id).unwrap();
    assert_eq!(content.files.iter().filter(|f| f.is_primary).count(), 1, "only one primary");
    assert_eq!(content.files[0].filename, "setup_gothic_1.0.2.exe", "primary listed first");
    assert_eq!(content.files.iter().find(|f| f.id == starter.id).unwrap().category, "installer");
    assert!(files::set_category(a.db.conn().unwrap(), id, starter.id, Some("nonsense")).is_err());

    // resolve for Open / Run / Open Folder
    let (p, _) = files::resolve_file(a.db.conn().unwrap(), &layout, id, setup.id).unwrap();
    assert!(p.ends_with("setup_gothic_1.0.2.exe"));
    let sub = files::resolve_subfolder(a.db.conn().unwrap(), &layout, id, Some("Soundtrack")).unwrap();
    assert!(sub.ends_with("Soundtrack"));
    assert!(matches!(files::resolve_subfolder(a.db.conn().unwrap(), &layout, id, Some("../..")), Err(CoreError::PathOutsideAllowed)));

    // a tampered record pointing outside the folder is refused
    touch(&layout.games_dir().join("secret.txt"), 3);
    a.db.conn().unwrap().execute(
        "INSERT INTO files (game_id, file_path, filename, file_type) VALUES (?1, '../secret.txt', 'secret.txt', 'text')",
        [id],
    ).unwrap();
    let evil: i64 = a.db.conn().unwrap().query_row("SELECT id FROM files WHERE filename = 'secret.txt'", [], |r| r.get(0)).unwrap();
    assert!(matches!(files::resolve_file(a.db.conn().unwrap(), &layout, id, evil), Err(CoreError::PathOutsideAllowed)));

    #[cfg(unix)]
    {
        // symlink escaping the game folder
        std::os::unix::fs::symlink(layout.games_dir().join("secret.txt"), folder.join("link.txt")).unwrap();
        let c = files::archive_content(a.db.conn().unwrap(), &layout, id).unwrap();
        let link = c.files.iter().find(|f| f.filename == "link.txt");
        if let Some(link) = link {
            assert!(matches!(files::resolve_file(a.db.conn().unwrap(), &layout, id, link.id), Err(CoreError::PathOutsideAllowed)));
        }
    }
}

#[test]
fn missing_files_and_unavailable_folders_keep_their_records() {
    let fx = Fixture::new();
    let mut a = fx.open("D");
    let layout = a.layout.clone();
    let folder = fx.drive("D").join("Ext").join("Thief");
    make_game_folder(&folder, "thief");
    let id = add_game(&mut a, "Thief", &folder);
    let before = files::archive_content(a.db.conn().unwrap(), &layout, id).unwrap().files.len();

    fs::remove_file(folder.join("Manual.pdf")).unwrap();
    let c = files::archive_content(a.db.conn().unwrap(), &layout, id).unwrap();
    assert_eq!(c.files.len(), before, "the record of a missing file is kept");
    assert!(c.files.iter().find(|f| f.filename == "Manual.pdf").unwrap().is_missing);

    // the folder disappears (drive unplugged)
    fs::rename(fx.drive("D").join("Ext"), fx.drive("D").join("Ext_gone")).unwrap();
    let report = games::check_availability(a.db.conn().unwrap(), &layout.root).unwrap();
    assert_eq!(report.unavailable_titles, vec!["Thief"]);
    let c = files::archive_content(a.db.conn().unwrap(), &layout, id).unwrap();
    assert!(!c.folder_available);
    assert_eq!(c.files.len(), before, "file records stay while the folder is unavailable");
    assert_eq!(games::game_count(a.db.conn().unwrap()).unwrap(), 1, "the game record is never deleted automatically");
    let listed = games::list_games(a.db.conn().unwrap(), &LibraryQuery::default(), "TEST-PC").unwrap();
    assert!(listed.len() == 1 && !listed[0].is_available, "the offline game stays listed, marked unavailable");
    assert_eq!(games::library_facets(a.db.conn().unwrap()).unwrap().unavailable, 1);

    // editing a game whose folder is offline still works
    let mut inp = input("Thief: The Dark Project", &folder);
    inp.description = "edited offline".into();
    games::update_game(a.db.conn().unwrap(), &layout, id, &inp).unwrap();

    fs::rename(fx.drive("D").join("Ext_gone"), fx.drive("D").join("Ext")).unwrap();
    assert_eq!(games::check_availability(a.db.conn().unwrap(), &layout.root).unwrap().unavailable, 0);
}

// ------------------------------------------------------------- Test 5

#[test]
fn t5_journal_persists_across_restarts() {
    let fx = Fixture::new();
    let mut a = fx.open("D");
    let layout = a.layout.clone();
    let f1 = layout.games_dir().join("A");
    let f2 = layout.games_dir().join("B");
    fs::create_dir_all(&f1).unwrap();
    fs::create_dir_all(&f2).unwrap();
    let g1 = add_game(&mut a, "Alpha", &f1);
    let g2 = add_game(&mut a, "Beta", &f2);
    let mk = |gid, date: &str, title: &str| JournalInput { game_id: gid, entry_date: date.into(), title: title.into(), content: format!("<h2>{title}</h2><ul><li>x</li></ul>") };
    let e1 = journal::create_entry(a.db.conn().unwrap(), &mk(g1, "2026-11-10", "First")).unwrap();
    journal::write_mirror(&layout, &e1).unwrap();
    journal::create_entry(a.db.conn().unwrap(), &mk(g2, "2026-11-12", "Second")).unwrap();
    journal::create_entry(a.db.conn().unwrap(), &mk(g1, "2026-11-15", "Third")).unwrap();

    let (edited, prev_game) = journal::update_entry(a.db.conn().unwrap(), e1.id, &mk(g1, "2026-11-11", "First (edited)")).unwrap();
    assert_eq!(prev_game, g1);
    journal::write_mirror(&layout, &edited).unwrap();
    let mirror = layout.journals_dir().join(format!("game-{g1}")).join(format!("entry-{}.html", e1.id));
    assert!(fs::read_to_string(&mirror).unwrap().contains("First (edited)"));
    a.db.close();
    drop(a);

    let mut b = fx.open("D");
    let all = journal::list_entries(b.db.conn().unwrap(), &JournalQuery::default()).unwrap();
    assert_eq!(all.iter().map(|e| e.title.as_str()).collect::<Vec<_>>(), vec!["Third", "Second", "First (edited)"]);
    let asc = journal::list_entries(b.db.conn().unwrap(), &JournalQuery { order: Some("asc".into()), ..Default::default() }).unwrap();
    assert_eq!(asc[0].title, "First (edited)");
    let only_g1 = journal::list_entries(b.db.conn().unwrap(), &JournalQuery { game_id: Some(g1), limit: Some(1), ..Default::default() }).unwrap();
    assert_eq!(only_g1.len(), 1);
    assert_eq!(only_g1[0].title, "Third");
    let e = journal::get_entry(b.db.conn().unwrap(), e1.id).unwrap();
    assert_eq!(e.entry_date, "2026-11-11");
    assert!(e.content.contains("<ul><li>x</li></ul>"));

    let empty = journal::create_entry(b.db.conn().unwrap(), &JournalInput { game_id: g1, entry_date: "2026-11-11".into(), ..Default::default() });
    assert!(matches!(empty, Err(CoreError::Invalid(_))));
    let deleted = journal::delete_entry(b.db.conn().unwrap(), e1.id).unwrap();
    journal::remove_mirror(&b.layout, deleted.game_id, deleted.id);
    assert!(!mirror.exists());
}

// ------------------------------------------------------------- Test 6

#[test]
fn t6_scanner_detects_imports_selected_and_avoids_duplicates() {
    let fx = Fixture::new();
    let mut a = fx.open("D");
    let layout = a.layout.clone();
    let root = fx.drive("D").join("GOG_Installers");
    make_game_folder(&root.join("Game_A"), "game_a");
    // installers in generic subfolders -> the parent is the game
    touch(&root.join("Baldurs_Gate_3_[GOG]").join("installer").join("setup_baldurs_gate_3.exe"), 10);
    touch(&root.join("Baldurs_Gate_3_[GOG]").join("extras").join("manual.pdf"), 10);
    // nested one level deeper
    make_game_folder(&root.join("Strategy").join("Heroes 3"), "heroes3");
    // title from a text file
    touch(&root.join("unk_folder_01").join("setup.exe"), 10);
    fs::write(root.join("unk_folder_01").join("title.txt"), "Planescape: Torment\n").unwrap();
    // title from goggame-*.info
    touch(&root.join("x").join("setup_x.exe"), 10);
    fs::write(root.join("x").join("goggame-1207.info"), r#"{"name": "Arcanum: Of Steamworks"}"#).unwrap();
    // archives only
    touch(&root.join("Doom (1993)").join("doom.zip"), 10);
    // not a game
    touch(&root.join("Docs").join("readme.txt"), 10);

    let cancel = AtomicBool::new(false);
    let mut progress_calls = 0;
    let mut result = scanner::scan(&root, &layout.root, 3, &cancel, &mut |_| progress_calls += 1).unwrap();
    assert!(!result.cancelled);
    assert!(progress_calls >= 1);
    scanner::annotate(a.db.conn().unwrap(), &mut result.candidates).unwrap();
    let mut titles: Vec<_> = result.candidates.iter().map(|c| c.title.clone()).collect();
    titles.sort();
    assert_eq!(titles, vec!["Arcanum: Of Steamworks", "Baldurs Gate 3", "Doom", "Game A", "Heroes 3", "Planescape: Torment"]);
    let bg3 = result.candidates.iter().find(|c| c.title == "Baldurs Gate 3").unwrap();
    assert!(bg3.folder_path.ends_with("Baldurs_Gate_3_[GOG]"));
    assert_eq!(bg3.stored_path, "../GOG_Installers/Baldurs_Gate_3_[GOG]");
    let game_a = result.candidates.iter().find(|c| c.title == "Game A").unwrap();
    assert_eq!(game_a.main_installer.as_deref(), Some("setup_game_a_1.0.2.exe"));
    assert!(game_a.categories.iter().any(|c| c.category == "music" && c.count == 2));
    assert_eq!(result.candidates.iter().find(|c| c.title == "Doom").unwrap().confidence, "weak");
    assert!(result.candidates.iter().all(|c| c.status == "new"));

    // import only some of them
    let pick = |t: &str| {
        let c = result.candidates.iter().find(|c| c.title == t).unwrap();
        ImportItem { folder_path: c.folder_path.clone(), title: c.title.clone(), ..Default::default() }
    };
    let report = scanner::import(a.db.conn().unwrap(), &layout, &[pick("Game A"), pick("Heroes 3")]).unwrap();
    assert_eq!(report.imported.len(), 2);
    assert!(report.skipped.is_empty());
    let id_a = report.imported[0].id;
    let content = files::archive_content(a.db.conn().unwrap(), &layout, id_a).unwrap();
    assert!(content.files.iter().any(|f| f.is_primary && f.filename == "setup_game_a_1.0.2.exe"), "obvious installer pre-selected");

    // re-scan: imported ones are recognised, importing again creates no duplicates
    let mut again = scanner::scan(&root, &layout.root, 3, &cancel, &mut |_| {}).unwrap();
    scanner::annotate(a.db.conn().unwrap(), &mut again.candidates).unwrap();
    assert_eq!(again.candidates.iter().filter(|c| c.status == "in_catalog").count(), 2);
    let report = scanner::import(a.db.conn().unwrap(), &layout, &[pick("Game A"), pick("Planescape: Torment")]).unwrap();
    assert_eq!(report.imported.len(), 1);
    assert_eq!(report.skipped.len(), 1);
    assert!(report.skipped[0].reason.contains("already assigned"));
    assert_eq!(games::game_count(a.db.conn().unwrap()).unwrap(), 3);

    // same title in a different folder -> flagged as possible duplicate
    make_game_folder(&fx.drive("D").join("Other").join("Game A"), "game_a");
    let mut other = scanner::scan(&fx.drive("D").join("Other"), &layout.root, 3, &cancel, &mut |_| {}).unwrap();
    scanner::annotate(a.db.conn().unwrap(), &mut other.candidates).unwrap();
    assert_eq!(other.candidates[0].status, "possible_duplicate");
}

#[test]
fn scanner_can_be_cancelled_and_handles_single_game_root() {
    let fx = Fixture::new();
    let a = fx.open("D");
    let root = fx.drive("D").join("Many");
    for i in 0..20 {
        make_game_folder(&root.join(format!("Game {i}")), &format!("g{i}"));
    }
    let cancel = AtomicBool::new(true);
    let r = scanner::scan(&root, &a.layout.root, 3, &cancel, &mut |_| {}).unwrap();
    assert!(r.cancelled);
    assert!(r.candidates.is_empty());

    // choosing a single game folder yields exactly that game
    let cancel = AtomicBool::new(false);
    let r = scanner::scan(&root.join("Game 3"), &a.layout.root, 3, &cancel, &mut |_| {}).unwrap();
    assert_eq!(r.candidates.len(), 1);
    assert_eq!(r.candidates[0].title, "Game 3");
    assert!(scanner::scan(&root.join("missing"), &a.layout.root, 3, &cancel, &mut |_| {}).is_err());
}

// ------------------------------------------------------------- Test 7

#[test]
fn t7_unplugged_database_is_reported_never_deleted() {
    let fx = Fixture::new();
    let mut a = fx.open("D");
    let folder = a.layout.games_dir().join("G");
    fs::create_dir_all(&folder).unwrap();
    add_game(&mut a, "G", &folder);
    a.db.close();

    // "unplug": the archive disappears while the app runs
    fs::rename(fx.drive("D"), fx.drive("D_unplugged")).unwrap();
    assert!(!a.layout.is_available());
    assert!(matches!(a.db.conn(), Err(CoreError::ArchiveUnavailable)));
    // "plug back in"
    fs::rename(fx.drive("D_unplugged"), fx.drive("D")).unwrap();
    assert_eq!(games::game_count(a.db.conn().unwrap()).unwrap(), 1, "records are intact");
}

// ------------------------------------------------------------- artwork & backups

#[test]
fn artwork_is_optimized_and_validated() {
    let fx = Fixture::new();
    let a = fx.open("D");
    let big = fx.tmp.path().join("big.png");
    write_png(&big, 2400, 3600);
    let art = images::process_artwork(&a.layout, &big).unwrap();
    assert!(art.width <= 1200 && art.height <= 1600);
    let thumb = image::open(a.layout.root.join(&art.thumbnail_path)).unwrap();
    assert_eq!((thumb.width(), thumb.height()), (360, 480), "thumbnail = what the 3:4 frame shows");
    assert!(art.artwork_path.ends_with(".webp") && art.thumbnail_path.starts_with("Data/Thumbnails/"));
    assert!(fs::metadata(&big).unwrap().len() > 0, "source untouched");

    // a webp source works too
    let webp_src = fx.tmp.path().join("src.webp");
    fs::copy(a.layout.root.join(&art.artwork_path), &webp_src).unwrap();
    assert!(images::process_artwork(&a.layout, &webp_src).is_ok());

    let txt = fx.tmp.path().join("cover.gif");
    fs::write(&txt, b"GIF89a").unwrap();
    assert!(matches!(images::process_artwork(&a.layout, &txt), Err(CoreError::Invalid(_))));
    let broken = fx.tmp.path().join("broken.png");
    fs::write(&broken, b"not an image").unwrap();
    assert!(images::process_artwork(&a.layout, &broken).is_err());

    // none of the processed images is referenced by a game -> orphans
    let mut a = a;
    let removed = images::cleanup_orphans(a.db.conn().unwrap(), &a.layout).unwrap();
    assert_eq!(removed, 4);

    // the image endpoint refuses anything outside Data/Artwork and Data/Thumbnails
    assert!(images::read_archive_image(&a.layout, "Data/library.db").is_err());
    assert!(images::read_archive_image(&a.layout, "Data/Thumbnails/../library.db").is_err());
}

#[test]
fn backups_auto_prune_and_restore() {
    let fx = Fixture::new();
    let mut a = fx.open("D");
    let layout = a.layout.clone();
    assert!(backup::auto_backup_if_due(a.db.conn().unwrap(), &layout, 10).unwrap().is_none(), "empty catalog is not backed up");
    let f1 = layout.games_dir().join("One");
    fs::create_dir_all(&f1).unwrap();
    add_game(&mut a, "One", &f1);

    let auto = backup::auto_backup_if_due(a.db.conn().unwrap(), &layout, 10).unwrap();
    assert!(auto.is_some());
    assert!(backup::auto_backup_if_due(a.db.conn().unwrap(), &layout, 10).unwrap().is_none(), "not due again within 24 h");

    let manual = backup::create_backup(a.db.conn().unwrap(), &layout, backup::BackupKind::Manual).unwrap();
    assert_eq!(backup::verify(&layout.backups_dir().join(&manual.file_name)).unwrap().games, 1);

    for _ in 0..4 {
        backup::create_backup(a.db.conn().unwrap(), &layout, backup::BackupKind::Auto).unwrap();
    }
    backup::prune_auto_backups(&layout, 2).unwrap();
    let list = backup::list_backups(&layout).unwrap();
    assert_eq!(list.iter().filter(|b| b.kind == "auto").count(), 2);
    assert_eq!(list.iter().filter(|b| b.kind == "manual").count(), 1, "manual backups are never pruned");

    // change data, then restore the manual backup
    let f2 = layout.games_dir().join("Two");
    fs::create_dir_all(&f2).unwrap();
    add_game(&mut a, "Two", &f2);
    assert_eq!(games::game_count(a.db.conn().unwrap()).unwrap(), 2);
    let rep = backup::restore(&mut a.db, &layout, &manual.file_name).unwrap();
    assert_eq!(rep.games, 1);
    assert_eq!(games::game_count(a.db.conn().unwrap()).unwrap(), 1);
    assert!(layout.backups_dir().join(&rep.safety_backup).is_file(), "safety backup before restore");
    assert!(f2.is_dir(), "restoring never touches game folders");

    assert!(matches!(backup::restore(&mut a.db, &layout, "../library.db"), Err(CoreError::PathOutsideAllowed)));
    fs::write(layout.backups_dir().join("library-20990101-000000-manual.db"), b"garbage").unwrap();
    assert!(backup::restore(&mut a.db, &layout, "library-20990101-000000-manual.db").is_err());
    assert_eq!(games::game_count(a.db.conn().unwrap()).unwrap(), 1, "failed restore keeps the current data");
}

#[test]
fn config_roundtrip_and_corruption() {
    let fx = Fixture::new();
    let a = fx.open("D");
    let path = a.layout.config_path();
    let mut cfg = curator_core::config::load(&path);
    assert_eq!(cfg.view_mode, "grid");
    assert_eq!(cfg.theme, "classic");
    cfg.view_mode = "catalog".into();
    cfg.theme = "amber".into();
    cfg.scan_root = Some(paths::to_portable(&a.layout.root, &a.layout.games_dir()));
    curator_core::config::save(&path, &cfg).unwrap();
    assert_eq!(curator_core::config::load(&path), cfg);
    // a config.json from before themes existed still loads (missing field -> default)
    fs::write(&path, r#"{"viewMode":"catalog","wizardCompleted":true}"#).unwrap();
    let old = curator_core::config::load(&path);
    assert_eq!((old.theme.as_str(), old.view_mode.as_str(), old.wizard_completed), ("classic", "catalog", true));
    assert_eq!(old.profile_split, curator_core::config::PROFILE_SPLIT_DEFAULT, "divider of the profile: default");
    assert!(old.show_welcome, "welcome window shown unless turned off");
    let mut dos = cfg.clone();
    dos.theme = "dos".into();
    dos.show_welcome = false;
    let dos = dos.validate().unwrap();
    assert!(dos.theme == "dos" && !dos.show_welcome);
    // the profile divider stays within 25-75 %
    let mut wide = cfg.clone();
    wide.profile_split = 90.0;
    assert_eq!(wide.validate().unwrap().profile_split, 75.0);
    fs::write(&path, r#"{"profileSplit":12}"#).unwrap();
    assert_eq!(curator_core::config::load(&path).profile_split, 25.0);
    fs::write(&path, r#"{"profileSplit":61.5}"#).unwrap();
    assert_eq!(curator_core::config::load(&path).profile_split, 61.5);
    let mut bad = cfg.clone();
    bad.theme = "neon".into();
    assert!(bad.validate().is_err());
    // options removed in later versions do not reset the rest of the settings
    fs::write(&path, r#"{"theme":"amber","viewMode":"catalog","startScreen":"journal","sort":"developer","cardSize":"large"}"#).unwrap();
    let legacy = curator_core::config::load(&path);
    assert_eq!((legacy.theme.as_str(), legacy.view_mode.as_str(), legacy.card_size.as_str(), legacy.sort.as_str()), ("amber", "catalog", "large", "title"));
    fs::write(&path, "{ broken").unwrap();
    assert_eq!(curator_core::config::load(&path).view_mode, "grid");
    assert!(path.with_extension("json.bad").is_file());
}

// ------------------------------------------------------------- Test 8 (core part)

#[test]
fn t8_performance_with_500_games() {
    let fx = Fixture::new();
    let mut a = fx.open("D");
    let layout = a.layout.clone();
    let root = fx.drive("D").join("Library500");
    let genres = ["RPG", "Strategy", "Action", "Adventure", "Shooter"];
    let t0 = Instant::now();
    for i in 0..500 {
        let folder = root.join(format!("Game {i:03}"));
        touch(&folder.join(format!("setup_game_{i}.exe")), 16);
        let inp = GameInput {
            title: format!("Game Title {i:03}"),
            folder_path: folder.to_string_lossy().into(),
            genre: genres[i % 5].into(),
            release_year: Some(1990 + (i % 35) as i64),
            tags: vec![format!("tag{}", i % 12)],
            ..Default::default()
        };
        games::create_game(a.db.conn().unwrap(), &layout, &inp).unwrap();
    }
    let insert_ms = t0.elapsed().as_millis();
    let c = a.db.conn().unwrap();

    let t = Instant::now();
    let all = games::list_games(c, &LibraryQuery::default(), "TEST-PC").unwrap();
    let list_ms = t.elapsed().as_secs_f64() * 1000.0;
    assert_eq!(all.len(), 500);

    let t = Instant::now();
    let found = games::list_games(c, &LibraryQuery { search: Some("title 042".into()), ..Default::default() }, "TEST-PC").unwrap();
    let search_ms = t.elapsed().as_secs_f64() * 1000.0;
    assert_eq!(found.len(), 1);

    let t = Instant::now();
    let by_size = games::list_games(c, &LibraryQuery { sort: Some("size".into()), ..Default::default() }, "TEST-PC").unwrap();
    let filter_ms = t.elapsed().as_secs_f64() * 1000.0;
    assert_eq!(by_size.len(), 500);

    let t = Instant::now();
    games::library_facets(c).unwrap();
    let facets_ms = t.elapsed().as_secs_f64() * 1000.0;

    let t = Instant::now();
    games::check_availability(c, &layout.root).unwrap();
    let avail_ms = t.elapsed().as_secs_f64() * 1000.0;

    let cancel = AtomicBool::new(false);
    let t = Instant::now();
    let scan = scanner::scan(&root, &layout.root, 3, &cancel, &mut |_| {}).unwrap();
    let scan_ms = t.elapsed().as_millis();
    assert_eq!(scan.candidates.len(), 500);

    println!(
        "PERF 500 games: insert {insert_ms} ms total | list all {list_ms:.2} ms | search {search_ms:.2} ms | \
         sort by size {filter_ms:.2} ms | facets {facets_ms:.2} ms | availability check {avail_ms:.2} ms | scan 500 folders {scan_ms} ms"
    );
    // generous ceilings - debug build on a shared machine
    assert!(list_ms < 200.0 && search_ms < 100.0 && facets_ms < 100.0);
}


// ------------------------------------------------------------- artwork crop

fn two_colour_png(path: &std::path::Path) {
    // 800x400: left half red, right half blue
    let img = image::RgbImage::from_fn(800, 400, |x, _| if x < 400 { image::Rgb([220, 20, 20]) } else { image::Rgb([20, 20, 220]) });
    img.save(path).unwrap();
}

fn px(layout: &archive::ArchiveLayout, rel: &str, x: u32, y: u32) -> [u8; 3] {
    let img = image::open(layout.root.join(rel)).unwrap().to_rgb8();
    assert_eq!(img.dimensions(), (360, 480));
    img.get_pixel(x, y).0
}

fn reddish(c: [u8; 3]) -> bool {
    c[0] > 150 && c[2] < 90
}
fn bluish(c: [u8; 3]) -> bool {
    c[2] > 150 && c[0] < 90
}

#[test]
fn artwork_crop_is_rendered_and_can_be_changed() {
    let fx = Fixture::new();
    let mut a = fx.open("D");
    let src = fx.tmp.path().join("wide.png");
    two_colour_png(&src);
    let folder = a.layout.games_dir().join("Wide");
    fs::create_dir_all(&folder).unwrap();

    // default: the centred 3:4 part - half red, half blue
    let art = images::process_artwork(&a.layout, &src).unwrap();
    assert!(reddish(px(&a.layout, &art.thumbnail_path, 40, 240)) && bluish(px(&a.layout, &art.thumbnail_path, 320, 240)));
    assert_eq!(Crop::centred(800, 400), Crop { x: 0.3125, y: 0.0, w: 0.375, h: 1.0 });

    // chosen when adding: the left part (red only); the first thumbnail is replaced
    let mut inp = input("Wide", &folder);
    let left = Crop { x: 0.0, y: 0.0, w: 0.375, h: 1.0 };
    inp.artwork = Some(ArtworkChange::Set { artwork_path: art.artwork_path.clone(), thumbnail_path: art.thumbnail_path.clone(), crop: Some(left) });
    let id = games::create_game(a.db.conn().unwrap(), &a.layout, &inp).unwrap();
    let g = games::get_game(a.db.conn().unwrap(), &a.layout, id).unwrap();
    assert_eq!(g.cover_crop, Some(left));
    let t1 = g.thumbnail_path.clone().unwrap();
    assert_ne!(t1, art.thumbnail_path);
    assert!(!a.layout.root.join(&art.thumbnail_path).exists(), "superseded thumbnail removed");
    assert!(reddish(px(&a.layout, &t1, 20, 20)) && reddish(px(&a.layout, &t1, 340, 460)));

    // adjusted later in the profile: the right part (blue only)
    let right = Crop { x: 0.625, y: 0.0, w: 0.375, h: 1.0 };
    let t2 = games::set_cover_crop(a.db.conn().unwrap(), &a.layout, id, &right).unwrap();
    assert!(bluish(px(&a.layout, &t2, 20, 20)) && bluish(px(&a.layout, &t2, 340, 460)));
    assert!(!a.layout.root.join(&t1).exists());
    assert!(a.layout.root.join(&art.artwork_path).is_file(), "the whole image is kept for later changes");

    // zoomed out so the whole picture fits: dark bands above and below
    let whole = Crop { x: 0.0, y: -0.8333, w: 1.0, h: 2.6667 };
    let mut edit = input("Wide", &folder);
    edit.artwork = Some(ArtworkChange::Crop { crop: whole });
    games::update_game(a.db.conn().unwrap(), &a.layout, id, &edit).unwrap();
    let g = games::get_game(a.db.conn().unwrap(), &a.layout, id).unwrap();
    let t3 = g.thumbnail_path.unwrap();
    let band = px(&a.layout, &t3, 180, 20);
    assert!(band.iter().all(|&v| v < 90), "letterbox band is dark: {band:?}");
    assert!(reddish(px(&a.layout, &t3, 60, 240)) && bluish(px(&a.layout, &t3, 300, 240)));
    assert!(!a.layout.root.join(&t2).exists());

    // editing without touching the artwork keeps the crop
    games::update_game(a.db.conn().unwrap(), &a.layout, id, &input("Wide", &folder)).unwrap();
    assert_eq!(games::get_game(a.db.conn().unwrap(), &a.layout, id).unwrap().cover_crop, Some(whole));

    // invalid crops are refused
    for bad in [Crop { x: f64::NAN, y: 0.0, w: 0.5, h: 0.5 }, Crop { x: 2.0, y: 0.0, w: 0.5, h: 0.5 }, Crop { x: 0.0, y: 0.0, w: 0.0, h: 1.0 }] {
        assert!(games::set_cover_crop(a.db.conn().unwrap(), &a.layout, id, &bad).is_err());
    }
    // a game without artwork has nothing to adjust
    let plain_dir = a.layout.games_dir().join("Plain");
    fs::create_dir_all(&plain_dir).unwrap();
    let plain = add_game(&mut a, "Plain", &plain_dir);
    assert!(games::set_cover_crop(a.db.conn().unwrap(), &a.layout, plain, &left).is_err());
}

// ------------------------------------------------------------- installed games

#[test]
fn installed_games_are_detected_per_computer() {
    let fx = Fixture::new();
    let mut a = fx.open("D");
    let gog = a.layout.games_dir();
    let mk = |a: &mut OpenedArchive, title: &str, installer: &str| {
        let dir = gog.join(title.replace('\'', ""));
        fs::create_dir_all(&dir).unwrap();
        touch(&dir.join(installer), 10);
        let id = add_game(a, title, &dir);
        files::archive_content(a.db.conn().unwrap(), &a.layout, id).unwrap();
        id
    };
    let ss2 = mk(&mut a, "System Shock 2", "setup_system_shock_2_2.48_(gog-3)_(12345).exe");
    let bg3 = mk(&mut a, "Baldur's Gate 3", "setup_bg3_v4.1.exe");
    let gothic = mk(&mut a, "Gothic", "setup_gothic_2.6.0.0.exe");
    let mafia = mk(&mut a, "Mafia", "setup_mafia_1.2.exe");

    // installations on "drive C"
    let c = fx.drive("C");
    let ss2_dir = c.join("GOG Games").join("SS2 install");
    fs::create_dir_all(&ss2_dir).unwrap();
    touch(&ss2_dir.join("SS2.exe"), 10);
    touch(&ss2_dir.join("unins000.exe"), 10);
    fs::write(
        ss2_dir.join("goggame-1207659187.info"),
        r#"{"gameId":"1207659187","name":"System Shock 2","playTasks":[
            {"category":"tool","isPrimary":false,"path":"..\\..\\evil.exe","type":"FileTask"},
            {"category":"game","isPrimary":true,"path":"SS2.exe","arguments":"-nosound","type":"FileTask"}]}"#,
    )
    .unwrap();
    let bg3_dir = c.join("Games").join("Baldurs Gate 3");
    fs::create_dir_all(bg3_dir.join("bin")).unwrap();
    touch(&bg3_dir.join("bin").join("bg3.exe"), 10);
    let gothic_dir = c.join("Other").join("Gothic");
    fs::create_dir_all(gothic_dir.join("System")).unwrap();
    touch(&gothic_dir.join("System").join("Gothic.exe"), 10);

    let prog = |name: &str, dir: &std::path::Path, exe: &str, source: &str| InstalledProgram {
        name: name.into(),
        install_dir: dir.to_string_lossy().into(),
        exe: exe.into(),
        source: source.into(),
        ..Default::default()
    };
    let programs = vec![
        // GOG key; the name differs only in punctuation and the installer gives the same name
        prog("System Shock\u{2122} 2", &ss2_dir, "", "gog"),
        // Windows list of programs, program from DisplayIcon
        prog("Baldur's Gate 3", &bg3_dir, &bg3_dir.join("bin").join("bg3.exe").to_string_lossy(), "uninstall"),
        // another game with a similar name: no match
        prog("Gothic II", &gothic_dir, "", "uninstall"),
        prog("Mafia: The City of Lost Heaven II", &gothic_dir, "", "uninstall"),
        // listed, but its folder is not on this PC
        prog("Mafia - Classic Edition", &c.join("missing").join("Mafia"), "", "uninstall"),
        // unrelated software
        prog("7-Zip", &c, "", "uninstall"),
    ];
    let report = installs::refresh(a.db.conn().unwrap(), "test-pc", &programs).unwrap();
    assert_eq!((report.machine.as_str(), report.detected, report.manual), ("TEST-PC", 2, 0));

    let installed = |a: &mut OpenedArchive, machine: &str| -> Vec<String> {
        games::list_games(a.db.conn().unwrap(), &LibraryQuery::default(), machine)
            .unwrap()
            .into_iter()
            .filter(|g| g.is_installed)
            .map(|g| g.title)
            .collect()
    };
    assert_eq!(installed(&mut a, "TEST-PC"), vec!["Baldur's Gate 3", "System Shock 2"]);
    assert!(installed(&mut a, "OTHER-PC").is_empty(), "an installation belongs to one computer");

    // GOG: program and arguments from goggame-*.info; the "..\" task is ignored
    let st = installs::status(a.db.conn().unwrap(), "TEST-PC", ss2).unwrap().unwrap();
    assert!(st.found && st.can_play && st.source == "gog" && st.exe.ends_with("SS2.exe"), "{st:?}");
    let t = installs::launch_target(a.db.conn().unwrap(), "TEST-PC", ss2).unwrap();
    assert!(t.exe.ends_with("SS2.exe") && t.arguments == "-nosound" && t.working_dir == fs::canonicalize(&ss2_dir).unwrap());
    let st = installs::status(a.db.conn().unwrap(), "TEST-PC", bg3).unwrap().unwrap();
    assert!(st.can_play && st.exe.ends_with("bg3.exe"), "{st:?}");
    assert!(installs::status(a.db.conn().unwrap(), "TEST-PC", gothic).unwrap().is_none());
    assert!(installs::status(a.db.conn().unwrap(), "TEST-PC", mafia).unwrap().is_none());

    // the user points to Gothic's program; its folder becomes the installation
    let gexe = gothic_dir.join("System").join("Gothic.exe");
    assert!(installs::set_manual(a.db.conn().unwrap(), "TEST-PC", gothic, &gothic_dir.join("readme.txt")).is_err());
    let st = installs::set_manual(a.db.conn().unwrap(), "TEST-PC", gothic, &gexe).unwrap();
    assert!(st.source == "manual" && st.can_play, "{st:?}");

    // next start: BG3 was uninstalled; the manual record is kept
    let report = installs::refresh(a.db.conn().unwrap(), "TEST-PC", &programs[..1]).unwrap();
    assert_eq!((report.detected, report.manual), (1, 1));
    assert_eq!(installed(&mut a, "TEST-PC"), vec!["Gothic", "System Shock 2"]);

    // the program disappeared: Play explains it; nothing is deleted
    fs::remove_file(&gexe).unwrap();
    assert!(matches!(installs::launch_target(a.db.conn().unwrap(), "TEST-PC", gothic), Err(CoreError::NotFound(_))));
    assert!(!installs::status(a.db.conn().unwrap(), "TEST-PC", gothic).unwrap().unwrap().can_play);
    installs::forget(a.db.conn().unwrap(), "TEST-PC", gothic).unwrap();
    assert!(installs::status(a.db.conn().unwrap(), "TEST-PC", gothic).unwrap().is_none());

    // removing a game from the catalog removes its installation records (not the files)
    games::remove_game(a.db.conn().unwrap(), &a.layout, ss2).unwrap();
    let n: i64 = a.db.conn().unwrap().query_row("SELECT COUNT(*) FROM installations", [], |r| r.get(0)).unwrap();
    assert_eq!(n, 0);
    assert!(ss2_dir.join("SS2.exe").is_file());
}


// ------------------------------------------------------------- screenshots

fn shot_png(path: &std::path::Path, w: u32, h: u32, rgb: [u8; 3]) {
    image::RgbImage::from_pixel(w, h, image::Rgb(rgb)).save(path).unwrap();
}

#[test]
fn screenshots_gallery_and_journal() {
    let fx = Fixture::new();
    let mut a = fx.open("D");
    let dir = a.layout.games_dir().join("Shots");
    fs::create_dir_all(&dir).unwrap();
    let id = add_game(&mut a, "Shots", &dir);
    let other_dir = a.layout.games_dir().join("Other");
    fs::create_dir_all(&other_dir).unwrap();
    let other = add_game(&mut a, "Other", &other_dir);

    let src = fx.tmp.path().join("2026-11-19 boss fight.png");
    shot_png(&src, 1280, 720, [200, 40, 40]);
    let before = fs::read(&src).unwrap();

    // gallery: copies stored as WebP, 16:9 preview, original untouched
    let s1 = screenshots::add(a.db.conn().unwrap(), &a.layout, id, None, &src).unwrap();
    assert!(s1.image_path.starts_with(&format!("Data/Screenshots/game-{id}/shot-")) && s1.image_path.ends_with(".webp"));
    assert_eq!((s1.width, s1.height), (1280, 720));
    assert_eq!(s1.original_name, "2026-11-19 boss fight.png");
    assert!(s1.taken_at.is_some(), "time of the original file");
    let thumb = image::open(a.layout.root.join(&s1.thumbnail_path)).unwrap();
    assert_eq!((thumb.width(), thumb.height()), (480, 270));
    assert_eq!(fs::read(&src).unwrap(), before, "the original is only read");
    assert!(images::read_archive_image(&a.layout, &s1.image_path).is_ok(), "served to the UI");

    // a 4K picture stays 4K; a huge one is reduced to 3840 px
    let big = fx.tmp.path().join("big.png");
    shot_png(&big, 5000, 2000, [10, 10, 200]);
    let s2 = screenshots::add(a.db.conn().unwrap(), &a.layout, id, None, &big).unwrap();
    assert_eq!((s2.width, s2.height), (3840, 1536));

    // unsupported or broken files are refused
    let txt = fx.tmp.path().join("notes.txt");
    fs::write(&txt, "x").unwrap();
    assert!(screenshots::add(a.db.conn().unwrap(), &a.layout, id, None, &txt).is_err());

    // newest first; caption
    let list = screenshots::list_for_game(a.db.conn().unwrap(), id).unwrap();
    assert_eq!(list.iter().map(|s| s.id).collect::<Vec<_>>(), vec![s2.id, s1.id]);
    let s1 = screenshots::set_caption(a.db.conn().unwrap(), s1.id, "  The   hacker  ").unwrap();
    assert_eq!(s1.caption, "The hacker");
    assert!(screenshots::set_caption(a.db.conn().unwrap(), s1.id, &"x".repeat(501)).is_err());

    // attached to a journal entry: shown with the entry and in its HTML copy
    let entry = journal::create_entry(
        a.db.conn().unwrap(),
        &JournalInput { game_id: id, entry_date: "2026-11-19".into(), title: "Deck 4".into(), content: "<p>Finally.</p>".into() },
    )
    .unwrap();
    let touched = screenshots::attach(a.db.conn().unwrap(), &[s1.id], Some(entry.id)).unwrap();
    assert_eq!(touched, vec![entry.id]);
    let e = journal::get_entry(a.db.conn().unwrap(), entry.id).unwrap();
    assert_eq!(e.screenshots.len(), 1);
    assert_eq!(e.screenshots[0].caption, "The hacker");
    journal::write_mirror(&a.layout, &e).unwrap();
    let mirror = fs::read_to_string(a.layout.journals_dir().join(format!("game-{id}")).join(format!("entry-{}.html", e.id))).unwrap();
    let rel = s1.image_path.strip_prefix("Data/").unwrap();
    assert!(mirror.contains(&format!("../../{rel}")) && mirror.contains("The hacker"), "{mirror}");
    let s = screenshots::get(a.db.conn().unwrap(), s1.id).unwrap();
    assert_eq!((s.entry_title.as_deref(), s.entry_date.as_deref()), (Some("Deck 4"), Some("2026-11-19")));
    // only to entries of the same game; a screenshot can be added straight into an entry
    assert!(screenshots::attach(a.db.conn().unwrap(), &[s2.id], Some(entry.id)).is_ok());
    let src2 = fx.tmp.path().join("other.png");
    shot_png(&src2, 640, 360, [0, 200, 0]);
    let foreign = screenshots::add(a.db.conn().unwrap(), &a.layout, other, None, &src2).unwrap();
    assert!(screenshots::attach(a.db.conn().unwrap(), &[foreign.id], Some(entry.id)).is_err());
    assert!(screenshots::add(a.db.conn().unwrap(), &a.layout, other, Some(entry.id), &src2).is_err());
    assert_eq!(journal::get_entry(a.db.conn().unwrap(), entry.id).unwrap().screenshots.len(), 2);

    // detach: stays in the gallery
    screenshots::attach(a.db.conn().unwrap(), &[s2.id], None).unwrap();
    assert_eq!(journal::get_entry(a.db.conn().unwrap(), entry.id).unwrap().screenshots.len(), 1);
    assert_eq!(screenshots::list_for_game(a.db.conn().unwrap(), id).unwrap().len(), 2);

    // the entry moves to another game: its screenshot goes with it
    journal::update_entry(
        a.db.conn().unwrap(),
        entry.id,
        &JournalInput { game_id: other, entry_date: "2026-11-19".into(), title: "Deck 4".into(), content: "<p>Finally.</p>".into() },
    )
    .unwrap();
    assert_eq!(screenshots::get(a.db.conn().unwrap(), s1.id).unwrap().game_id, other);
    // deleting the entry keeps its screenshots
    journal::delete_entry(a.db.conn().unwrap(), entry.id).unwrap();
    let s = screenshots::get(a.db.conn().unwrap(), s1.id).unwrap();
    assert!(s.journal_entry_id.is_none());

    // delete one screenshot: record + stored copy; the original stays
    screenshots::delete(a.db.conn().unwrap(), &a.layout, s2.id).unwrap();
    assert!(!a.layout.root.join(&s2.image_path).exists() && !a.layout.root.join(&s2.thumbnail_path).exists());
    assert!(big.is_file());

    // start-up cleanup keeps screenshots, removes unfinished files and orphaned previews
    let dir_shots = a.layout.screenshots_dir().join(format!("game-{other}"));
    fs::write(dir_shots.join("shot-dead.tmp"), b"x").unwrap();
    fs::write(a.layout.thumbnails_dir().join("shot-orphan.webp"), b"x").unwrap();
    let kept = a.layout.screenshots_dir().join(format!("game-{id}")).join("shot-unknown.webp");
    fs::write(&kept, b"x").unwrap();
    images::cleanup_orphans(a.db.conn().unwrap(), &a.layout).unwrap();
    assert!(!dir_shots.join("shot-dead.tmp").exists() && !a.layout.thumbnails_dir().join("shot-orphan.webp").exists());
    assert!(kept.exists(), "screenshot pictures are never removed automatically");
    assert!(a.layout.root.join(&s1.image_path).exists());

    // removing a game removes its screenshot copies (the user is told how many)
    let preview = games::removal_preview(a.db.conn().unwrap(), &a.layout, other).unwrap();
    assert_eq!(preview.screenshots, 2);
    let opts = games::RemoveOptions { delete_screenshots: true, ..Default::default() };
    games::remove_game_with(a.db.conn().unwrap(), &a.layout, other, &opts).unwrap();
    assert!(!a.layout.root.join(&s1.image_path).exists() && !a.layout.root.join(&foreign.image_path).exists());
    assert!(src.is_file() && src2.is_file(), "originals untouched");
    // by default the screenshots stay in Data/Screenshots
    let s3 = screenshots::add(a.db.conn().unwrap(), &a.layout, id, None, &src).unwrap();
    games::remove_game(a.db.conn().unwrap(), &a.layout, id).unwrap();
    assert!(a.layout.root.join(&s3.image_path).is_file(), "kept unless the user chose to delete them");
}

// ------------------------------------------------------------- additional files

#[test]
fn additional_files_are_copied_or_linked() {
    let fx = Fixture::new();
    let mut a = fx.open("D");
    let dir = a.layout.games_dir().join("Old Game");
    fs::create_dir_all(&dir).unwrap();
    touch(&dir.join("setup_old_game.exe"), 10);
    let id = add_game(&mut a, "Old Game", &dir);
    let downloads = fx.drive("C").join("Downloads");
    fs::create_dir_all(&downloads).unwrap();
    let fix = downloads.join("widescreen fix.zip");
    fs::write(&fix, vec![7u8; 4096]).unwrap();
    let doc = downloads.join("Making of.mkv");
    fs::write(&doc, vec![1u8; 10_000]).unwrap();
    let inp = |c: &str, n: &str| AdditionInput { category: c.into(), note: n.into() };

    // copy into the archive: travels with the drive, original stays
    let a1 = additions::add_copy(a.db.conn().unwrap(), &a.layout, id, &fix, &inp("fix", "Needed on Windows 11")).unwrap();
    assert_eq!(a1.kind, "copy");
    assert_eq!(a1.path, format!("Data/Additions/game-{id}/widescreen fix.zip"));
    assert!(a1.exists && a1.portable && a1.size_bytes == 4096);
    assert_eq!(fs::read(a.layout.root.join(&a1.path)).unwrap(), fs::read(&fix).unwrap());
    assert!(fix.is_file(), "the original stays where it was");
    // the same name again: a free name
    let a1b = additions::add_copy(a.db.conn().unwrap(), &a.layout, id, &fix, &inp("fix", "")).unwrap();
    assert_eq!(a1b.filename, "widescreen fix (2).zip");

    // link outside the archive: absolute path, not portable
    let a2 = additions::add_link(a.db.conn().unwrap(), &a.layout, id, &doc, &inp("bonus", "Documentary")).unwrap();
    assert!(a2.kind == "link" && !a2.portable && a2.exists && std::path::Path::new(&a2.path).is_absolute(), "{a2:?}");
    assert!(additions::add_link(a.db.conn().unwrap(), &a.layout, id, &doc, &inp("bonus", "")).is_err(), "no duplicates");
    // link inside the archive: relative path, follows the drive
    let inside = a.layout.games_dir().join("Community Patch 1.2.zip");
    fs::write(&inside, b"patch").unwrap();
    let a3 = additions::add_link(a.db.conn().unwrap(), &a.layout, id, &inside, &inp("patch", "")).unwrap();
    assert!(a3.portable && a3.path == "Games/Community Patch 1.2.zip", "{a3:?}");

    // invalid input
    assert!(additions::add_copy(a.db.conn().unwrap(), &a.layout, id, &downloads, &inp("fix", "")).is_err(), "folder");
    assert!(additions::add_link(a.db.conn().unwrap(), &a.layout, id, &fix, &inp("cheat", "")).is_err(), "category");
    assert!(additions::add_copy(a.db.conn().unwrap(), &a.layout, 999, &fix, &inp("fix", "")).is_err(), "game");

    // ordered by kind: fix, patch, mod, bonus, other
    let list = additions::list(a.db.conn().unwrap(), &a.layout, id).unwrap();
    assert_eq!(list.iter().map(|x| x.category.as_str()).collect::<Vec<_>>(), vec!["fix", "fix", "patch", "bonus"]);
    let up = additions::update(a.db.conn().unwrap(), &a.layout, a2.id, &inp("bonus", "Making of, 45 min")).unwrap();
    assert_eq!(up.note, "Making of, 45 min");

    // a linked file that disappeared is shown as missing, never deleted from the list
    fs::rename(&doc, downloads.join("moved.mkv")).unwrap();
    let a2 = additions::get(a.db.conn().unwrap(), &a.layout, a2.id).unwrap();
    assert!(!a2.exists);
    assert!(matches!(additions::file(a.db.conn().unwrap(), &a.layout, a2.id), Err(CoreError::NotFound(_))));
    let exe = downloads.join("Patch 1.1.EXE");
    fs::write(&exe, b"MZ").unwrap();
    let ax = additions::add_link(a.db.conn().unwrap(), &a.layout, id, &exe, &inp("patch", "")).unwrap();
    assert!(ax.runnable && ax.risky && !a2.runnable && !a2.risky);
    additions::remove(a.db.conn().unwrap(), &a.layout, ax.id, true).unwrap();
    assert!(exe.is_file());

    // taking off the list: a link never touches the file, a copy only when asked
    additions::remove(a.db.conn().unwrap(), &a.layout, a3.id, true).unwrap();
    assert!(inside.is_file(), "a linked file is never deleted");
    additions::remove(a.db.conn().unwrap(), &a.layout, a1b.id, false).unwrap();
    assert!(a.layout.root.join(&a1b.path).is_file(), "copy kept when not asked");
    additions::remove(a.db.conn().unwrap(), &a.layout, a1.id, true).unwrap();
    assert!(!a.layout.root.join(&a1.path).exists(), "copy deleted on request");

    // removing the game (and choosing to delete copies) deletes the copies Game Curator made, nothing else
    let again = additions::add_copy(a.db.conn().unwrap(), &a.layout, id, &fix, &inp("fix", "")).unwrap();
    let preview = games::removal_preview(a.db.conn().unwrap(), &a.layout, id).unwrap();
    assert_eq!(preview.addition_copies, 1);
    let opts = games::RemoveOptions { delete_addition_copies: true, ..Default::default() };
    games::remove_game_with(a.db.conn().unwrap(), &a.layout, id, &opts).unwrap();
    assert!(!a.layout.root.join(&again.path).exists());
    assert!(fix.is_file() && dir.join("setup_old_game.exe").is_file());
}
