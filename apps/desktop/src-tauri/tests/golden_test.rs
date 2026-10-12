#![allow(clippy::unwrap_used, clippy::print_stderr)]

use std::fs;
use std::path::{Path, PathBuf};

/// Directory containing editor input/output fixtures
fn editor_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/editor")
}

fn golden_dir() -> PathBuf {
    editor_dir().join("golden")
}

fn read_input() -> String {
    fs::read_to_string(editor_dir().join("input.sii")).unwrap()
}

/// Load a golden file. If it doesn't exist, write `content` to it and return `content`.
/// This allows bootstrapping golden files on the first run.
fn golden_or_bootstrap(name: &str, content: &str) -> String {
    let path = golden_dir().join(name);
    if !path.exists() {
        fs::write(&path, content).unwrap();
        eprintln!("[bootstrap] wrote golden file: {}", path.display());
    }
    fs::read_to_string(&path).unwrap()
}

/// Compare actual output to golden file. Bootstraps if golden is missing.
fn assert_matches_golden(actual: &str, golden_name: &str) {
    let golden = golden_or_bootstrap(golden_name, actual);
    if actual != golden {
        let golden_path = golden_dir().join(golden_name);
        let diff_path = golden_dir().join(format!("{}.actual", golden_name));
        fs::write(&diff_path, actual).unwrap();
        panic!(
            "Output differs from golden file '{}'.\n\
             Expected {} bytes, got {} bytes.\n\
             Actual output written to: {}\n\
             If the new output is correct, run:\n\
               cp '{}' '{}'",
            golden_name,
            golden.len(),
            actual.len(),
            diff_path.display(),
            diff_path.display(),
            golden_path.display(),
        );
    }
}

// --- edit_money ---

#[test]
fn golden_edit_money() {
    let content = read_input();
    let mut doc = ets_vibes_lib::save_parser::sii::SiiDocument::new(content);
    doc.set_property("money_account", "99999").unwrap();
    assert_matches_golden(doc.content(), "edit_money.sii");
}

// --- edit_xp ---

#[test]
fn golden_edit_xp() {
    let content = read_input();
    let mut doc = ets_vibes_lib::save_parser::sii::SiiDocument::new(content);
    doc.set_property("experience_points", "5000").unwrap();
    assert_matches_golden(doc.content(), "edit_xp.sii");
}

// --- max_skills ---

#[test]
fn golden_max_skills() {
    let content = read_input();
    let mut doc = ets_vibes_lib::save_parser::sii::SiiDocument::new(content);
    for (name, value) in &[
        ("adr", "63"),
        ("long_dist", "6"),
        ("heavy", "6"),
        ("fragile", "6"),
        ("urgent", "6"),
        ("mechanical", "6"),
    ] {
        doc.set_property(name, value).unwrap();
    }
    assert_matches_golden(doc.content(), "max_skills.sii");
}

// --- repair_all ---

#[test]
fn golden_repair_all() {
    let content = read_input();
    let mut doc = ets_vibes_lib::save_parser::sii::SiiDocument::new(content);
    for name in &[
        "engine_wear",
        "engine_wear_unfixable",
        "transmission_wear",
        "transmission_wear_unfixable",
        "cabin_wear",
        "cabin_wear_unfixable",
        "chassis_wear",
        "chassis_wear_unfixable",
    ] {
        doc.set_property(name, "0").unwrap();
    }
    assert_matches_golden(doc.content(), "repair_all.sii");
}

// --- refuel_all ---

#[test]
fn golden_refuel_all() {
    let content = read_input();
    let mut doc = ets_vibes_lib::save_parser::sii::SiiDocument::new(content);
    doc.set_property("fuel_relative", "1.0").unwrap();
    assert_matches_golden(doc.content(), "refuel_all.sii");
}

// --- unlock_all_cities (through the real SaveEditor path) ---

/// Set up a SaveFile layout pointing at `input.sii` content, run the real
/// editor operation, and return the saved file content.
fn run_editor_action(
    action: impl FnOnce(
        &mut ets_vibes_lib::save_parser::editor::SaveEditor,
    ) -> Result<(), ets_vibes_lib::save_parser::error::SaveError>,
) -> String {
    use ets_vibes_lib::core::profile::SaveFile;
    use ets_vibes_lib::save_parser::editor::SaveEditor;
    let dir = tempfile::tempdir().unwrap();
    let game_sii_dir = dir.path().join("save").join("quicksave");
    fs::create_dir_all(&game_sii_dir).unwrap();
    fs::write(game_sii_dir.join("game.sii"), read_input()).unwrap();

    let mut editor = SaveEditor::new(SaveFile::new(
        dir.path().to_path_buf(),
        "quicksave".to_string(),
    ));
    editor.load().unwrap();
    action(&mut editor).unwrap();
    editor.save().unwrap();
    fs::read_to_string(game_sii_dir.join("game.sii")).unwrap()
}

#[test]
fn golden_unlock_cities() {
    let actual = run_editor_action(|editor| editor.unlock_all_cities().map(|_| ()));
    assert_matches_golden(&actual, "unlock_cities.sii");
}

// --- full workflow (all operations) ---

#[test]
fn golden_full_workflow() {
    // True end-to-end: every operation through the real SaveEditor,
    // exactly like the app invokes them.
    let actual = run_editor_action(|editor| {
        editor.edit_money(99999).map(|_| ())?;
        editor.edit_xp(5000).map(|_| ())?;
        editor.max_skills().map(|_| ())?;
        editor.repair_all().map(|_| ())?;
        editor.refuel_all().map(|_| ())?;
        editor.unlock_all_cities().map(|_| ())?;
        Ok(())
    });

    assert_matches_golden(&actual, "full_workflow.sii");
}

// --- structural integrity tests ---

/// Verify that after all edits, the document is still valid SII
#[test]
fn golden_structural_integrity() {
    let content = read_input();
    let mut doc = ets_vibes_lib::save_parser::sii::SiiDocument::new(content);

    // Apply all edits
    doc.set_property("money_account", "99999").unwrap();
    doc.set_property("experience_points", "5000").unwrap();
    for (name, value) in &[
        ("adr", "63"),
        ("long_dist", "6"),
        ("heavy", "6"),
        ("fragile", "6"),
        ("urgent", "6"),
        ("mechanical", "6"),
    ] {
        doc.set_property(name, value).unwrap();
    }
    for name in &[
        "engine_wear",
        "engine_wear_unfixable",
        "transmission_wear",
        "transmission_wear_unfixable",
        "cabin_wear",
        "cabin_wear_unfixable",
        "chassis_wear",
        "chassis_wear_unfixable",
    ] {
        doc.set_property(name, "0").unwrap();
    }
    doc.set_property("fuel_relative", "1.0").unwrap();

    // Verify properties are readable
    assert_eq!(doc.get_property("money_account").unwrap(), "99999");
    assert_eq!(doc.get_property("experience_points").unwrap(), "5000");
    assert_eq!(doc.get_property("adr").unwrap(), "63");
    assert_eq!(doc.get_property("engine_wear").unwrap(), "0");
    assert_eq!(doc.get_property("fuel_relative").unwrap(), "1.0");

    // Verify section boundaries are intact
    let content = doc.content();
    let open_braces = content.matches('{').count();
    let close_braces = content.matches('}').count();
    assert_eq!(
        open_braces, close_braces,
        "Mismatched braces: {} open vs {} close",
        open_braces, close_braces
    );

    // Verify SiiNunit wrapper is intact
    assert!(
        content.starts_with("SiiNunit\n{"),
        "Document should start with SiiNunit header"
    );
    assert!(
        content.trim_end().ends_with('}'),
        "Document should end with closing brace"
    );

    // Verify all sections still exist
    assert!(content.contains("bank : _nameless.7646.a028.4150"));
    assert!(content.contains("driver_player : driver.125"));
    assert!(content.contains("player : _nameless.7646.a03f.a2e0"));
    assert!(content.contains("vehicle : _nameless.4e43.34c0"));
    assert!(content.contains("garage : garage.berlin"));
}

/// Verify that no section headers or structural elements were lost during editing
#[test]
fn golden_no_data_loss() {
    let content = read_input();
    let mut doc = ets_vibes_lib::save_parser::sii::SiiDocument::new(content.clone());

    doc.set_property("money_account", "99999").unwrap();
    doc.set_property("experience_points", "5000").unwrap();
    for (name, value) in &[
        ("adr", "63"),
        ("long_dist", "6"),
        ("heavy", "6"),
        ("fragile", "6"),
        ("urgent", "6"),
        ("mechanical", "6"),
    ] {
        doc.set_property(name, value).unwrap();
    }
    for name in &[
        "engine_wear",
        "engine_wear_unfixable",
        "transmission_wear",
        "transmission_wear_unfixable",
        "cabin_wear",
        "cabin_wear_unfixable",
        "chassis_wear",
        "chassis_wear_unfixable",
    ] {
        doc.set_property(name, "0").unwrap();
    }
    doc.set_property("fuel_relative", "1.0").unwrap();

    let edited = doc.content();

    // All section types must still exist
    for sec in &[
        "economy : _nameless.7646.a03e.f410",
        "bank : _nameless.7646.a028.4150",
        "driver_player : driver.125",
        "player : _nameless.7646.a03f.a2e0",
        "vehicle : _nameless.4e43.34c0",
        "garage : garage.berlin",
    ] {
        assert!(edited.contains(sec), "Section '{}' disappeared", sec);
    }

    // Brace balance preserved
    let open_before = content.matches('{').count();
    let close_before = content.matches('}').count();
    let open_after = edited.matches('{').count();
    let close_after = edited.matches('}').count();
    assert_eq!(
        open_before, open_after,
        "Open braces changed: {} -> {}",
        open_before, open_after
    );
    assert_eq!(
        close_before, close_after,
        "Close braces changed: {} -> {}",
        close_before, close_after
    );

    // SiiNunit wrapper intact
    assert!(edited.starts_with("SiiNunit\n{"), "Header corrupted");
    assert!(edited.trim_end().ends_with('}'), "Footer corrupted");

    // All non-edited property lines should still be present byte-identical
    let edited_set: std::collections::HashSet<&str> = edited.lines().collect();
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("money_account:")
            || trimmed.starts_with("experience_points:")
            || trimmed.starts_with("adr:")
            || trimmed.starts_with("long_dist:")
            || trimmed.starts_with("heavy:")
            || trimmed.starts_with("fragile:")
            || trimmed.starts_with("urgent:")
            || trimmed.starts_with("mechanical:")
            || trimmed.starts_with("engine_wear:")
            || trimmed.starts_with("transmission_wear:")
            || trimmed.starts_with("cabin_wear:")
            || trimmed.starts_with("chassis_wear:")
            || trimmed.starts_with("engine_wear_unfixable:")
            || trimmed.starts_with("transmission_wear_unfixable:")
            || trimmed.starts_with("cabin_wear_unfixable:")
            || trimmed.starts_with("chassis_wear_unfixable:")
            || trimmed.starts_with("fuel_relative:")
            || trimmed.starts_with("visited_cities:")
            || trimmed.starts_with("visited_cities_count:")
        {
            continue; // edited fields may have changed
        }
        assert!(
            edited_set.contains(line.trim_end()),
            "Line disappeared or changed: '{}'",
            line.trim_end()
        );
    }
}
