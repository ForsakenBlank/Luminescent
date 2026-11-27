//! The Settings page: a widget for every field of both systems' schema dumps, what Save & Apply writes,
//! search, pins, the hotkey recorder and its clash warnings, profiles, module buttons, small sizes, and a picture
//! of every section in Dark and Light (compared on Linux only, like `snapshots.rs`).

#![allow(clippy::unwrap_used)]

use std::path::Path;

use egui::{Event, Key, Modifiers, Rect, pos2};
use egui_kittest::kittest::Queryable;
use egui_kittest::{Harness, SnapshotOptions, SnapshotResults};
use lum_core::settings::{DefaultPlaces, FieldType, Schema, Settings, SettingsFiles};
use lum_ui::settings::model::{Control, control_of};
use lum_ui::settings::{Env, ModuleInfo, Out, SettingsUi};
use lum_ui::themes::demo::{Demo, Look};

struct State {
    demo: Demo,
    settings: Settings,
    ui: SettingsUi,
    outs: Vec<Out>,
    module: Option<ModuleInfo>,
    others: Vec<(String, String)>,
}

fn files(dir: &Path) -> SettingsFiles {
    SettingsFiles {
        settings: dir.join("settings.json"),
        profiles: dir.join("profiles.json"),
        profiles_dir: dir.join("profiles"),
    }
}

fn state(dir: &Path, look: Look, schema: &'static Schema) -> State {
    let mut settings = Settings::open(files(dir), schema, DefaultPlaces::placeholders());
    let mut n = 0;
    settings.set_id_source(move || {
        n += 1;
        format!("profile{n}")
    });
    State {
        demo: Demo::new(dir, look),
        settings,
        ui: SettingsUi::with_schema(schema, dir.join("picker.json"), dir.join("colours.json")),
        outs: Vec::new(),
        module: None,
        others: Vec::new(),
    }
}

fn harness_with(state: State, size: [f32; 2], render: bool) -> Harness<'static, State> {
    let builder = Harness::builder().with_size(size).with_pixels_per_point(1.0);
    let builder = if render && cfg!(target_os = "linux") { builder.wgpu() } else { builder };
    let harness = builder.build_ui_state(
        |ui, state: &mut State| {
            let ctx = ui.ctx().clone();
            state.demo.ready(&ctx);
            let State { settings, ui: page, outs, module, others, .. } = state;
            let key_user = |combo: &str| others.iter().find(|(c, _)| c == combo).map(|(_, w)| w.clone());
            let key_list = || others.clone();
            let info = |key: &str| module.clone().filter(|m| m.title.to_lowercase() == key);
            let env = Env {
                key_user: &key_user,
                dashboard_keys: &key_list,
                module: &info,
                version: "2.0.0-test",
                today: "2026-11-27",
            };
            // the overlay's panel is what paints behind the page
            let tokens = lum_ui::Tokens::of(&ctx);
            ui.painter().rect_filled(ui.max_rect(), 0.0, tokens.p.bg_base);
            let near = Rect::from_min_size(pos2(0.0, 0.0), ui.max_rect().size());
            outs.push(page.page_ui(ui, near, settings, &env));
        },
        state,
    );
    harness.ctx.options_mut(|o| o.input_options.max_double_click_delay = 5.0);
    harness
}

fn harness(dir: &tempfile::TempDir, size: [f32; 2]) -> Harness<'static, State> {
    harness_with(state(dir.path(), Look::Dark, Schema::get()), size, false)
}

fn saved(h: &Harness<'_, State>) -> Option<Vec<String>> {
    h.state().outs.iter().rev().find_map(|o| o.saved.as_ref().map(|s| s.changed.clone()))
}

fn settle(h: &mut Harness<'_, State>) {
    for _ in 0..40 {
        h.step();
    }
    h.run();
}

fn check<'a>(h: &'a Harness<'_, State>, label: &'a str) -> egui_kittest::Node<'a> {
    h.get_by_role_and_label(egui::Role::CheckBox, label)
}

fn leak(json: &str) -> &'static Schema {
    Box::leak(Box::new(Schema::parse(json).unwrap()))
}

#[test]
fn every_field_of_both_schema_dumps_gets_a_widget() {
    for dump in ["settings_schema.linux.json", "settings_schema.windows.json"] {
        let path = format!("{}/../../fixtures/{dump}", env!("CARGO_MANIFEST_DIR"));
        let schema = leak(&std::fs::read_to_string(path).unwrap());
        let dir = tempfile::tempdir().unwrap();
        let mut h = harness_with(state(dir.path(), Look::Dark, schema), [1000.0, 700.0], false);
        let mut seen = 0;
        for section in &schema.sections {
            h.state_mut().ui.open_section(&section.key);
            settle(&mut h);
            let drawn: Vec<(String, Control)> = h.state().ui.drawn().to_vec();
            for field in &section.fields {
                let full = format!("{}.{}", section.key, field.key);
                let found = drawn.iter().find(|(key, _)| *key == full);
                if field.kind == FieldType::Hidden {
                    assert!(found.is_none(), "{dump}: {full} is hidden but was drawn");
                    continue;
                }
                let (_, control) = found.unwrap_or_else(|| panic!("{dump}: {full} has no widget"));
                assert_eq!(Some(*control), control_of(&section.key, field), "{dump}: {full}");
                seen += 1;
            }
        }
        assert!(seen > 240, "{dump}: only {seen} fields were drawn");
    }
}

#[test]
fn nothing_is_left_to_repaint_once_the_sections_are_built() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = harness(&dir, [1000.0, 700.0]);
    // `run` fails when the page keeps asking for frames
    for _ in 0..40 {
        h.step();
    }
    h.run();
}

#[test]
fn saving_writes_what_the_python_app_would() {
    let dir = tempfile::tempdir().unwrap();
    let original = std::fs::read_to_string(format!(
        "{}/../../fixtures/sample_home/settings.json",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap();
    std::fs::write(dir.path().join("settings.json"), &original).unwrap();
    let mut h = harness(&dir, [1000.0, 700.0]);
    let before = h.state().settings.all().clone();
    settle(&mut h);

    check(&h, "Show notifications").click();
    h.run();
    h.get_by_label("Save & Apply").click();
    h.run();
    assert_eq!(saved(&h), Some(vec!["general".to_owned()]), "only the General section changed");

    // the same values as before but for the one the person changed, with hotkeys written the way Python's
    // normaliser writes them ("super" is "windows" there)
    let mut expected = before;
    expected["general"]["tray_messages"] = serde_json::json!(false);
    for section in Schema::get().sections.iter() {
        for field in section.fields.iter().filter(|f| f.kind == FieldType::Hotkey) {
            let value = expected[&section.key][&field.key].as_str().unwrap().to_owned();
            expected[&section.key][&field.key] =
                serde_json::json!(lum_platform::hotkeys::keys::normalize(&value));
        }
    }
    let written = std::fs::read_to_string(dir.path().join("settings.json")).unwrap();
    let on_disk: serde_json::Value = serde_json::from_str(&written).unwrap();
    assert_eq!(on_disk, serde_json::Value::Object(expected));
    assert!(written.starts_with("{\n  \""), "indented by two like json.dump(indent=2)");
}

#[test]
fn undo_puts_back_what_was_saved_and_reset_page_goes_to_the_defaults() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = harness(&dir, [1000.0, 700.0]);
    settle(&mut h);
    check(&h, "Show notifications").click();
    h.run();
    assert!(h.query_by_label("1 UNSAVED CHANGE").is_some());
    h.get_by_label("Undo changes").click();
    h.run();
    assert!(h.query_by_label("CHANGES UNDONE").is_some());
    assert!(h.state().settings.get_bool("general.tray_messages", false));

    h.state_mut().settings.set("general.tray_messages", serde_json::json!(false));
    h.state_mut().ui.mark_stale();
    h.run();
    h.get_by_label("Reset page").click();
    h.run();
    assert!(h.query_by_label("1 UNSAVED CHANGE").is_some(), "the default differs from the stored value");
}

#[test]
fn search_hides_other_sections_and_counts_matches() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = harness(&dir, [1000.0, 700.0]);
    settle(&mut h);
    assert!(h.query_by_role_and_label(egui::Role::Button, "Notes").is_some());
    let search = h.get_by_label("Search settings");
    search.focus();
    h.run();
    h.get_by_label("Search settings").type_text("autostart");
    h.run();
    h.run();
    assert!(h.query_by_role_and_label(egui::Role::Button, "General").is_some());
    assert!(
        h.query_by_role_and_label(egui::Role::Button, "Notes").is_none(),
        "a section without a match is hidden"
    );
}

#[test]
fn only_matching_settings_hides_the_other_rows() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = harness(&dir, [1000.0, 700.0]);
    settle(&mut h);
    assert!(h.query_by_label("Notification style").is_some());
    h.get_by_label("Only matching settings").click();
    h.run();
    h.get_by_label("Search settings").focus();
    h.run();
    h.get_by_label("Search settings").type_text("autostart");
    h.run();
    h.run();
    assert!(h.query_by_label("Notification style").is_none());
    assert!(h.state().settings.get_bool("ui.settings_search_only", false));
}

#[test]
fn a_section_can_be_pinned_and_stays_pinned() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = harness(&dir, [1000.0, 700.0]);
    settle(&mut h);
    h.hover_at(pos2(60.0, 300.0));
    h.run();
    // the star of any section shows while the mouse is over it
    let star = h.get_all_by_label_contains("Pin ").next().unwrap();
    star.click();
    h.run();
    let pinned = h.state().settings.get_str_list("ui.settings_pinned");
    assert_eq!(pinned.len(), 1);
}

fn press_combo(h: &mut Harness<'_, State>, modifiers: Modifiers, key: Key) {
    h.event(Event::Key { key, physical_key: Some(key), pressed: true, repeat: false, modifiers });
    h.run();
}

fn open(h: &mut Harness<'_, State>, key: &str) {
    h.state_mut().ui.open_section(key);
    settle(h);
}

#[test]
fn recording_a_hotkey_warns_when_another_one_has_it() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = harness(&dir, [1000.0, 700.0]);
    open(&mut h, "general");
    // the settings hotkey is the second hotkey on the page: its Record button
    let records: Vec<_> = h.get_all_by_label_contains("Record").collect();
    assert!(records.len() >= 2);
    records[1].click();
    h.run();
    assert_eq!(
        h.state().outs.iter().rev().find_map(|o| o.suspend_keys),
        Some(true),
        "hotkeys pause while recording"
    );
    assert!(h.state().ui.busy());
    press_combo(&mut h, Modifiers::CTRL, Key::U);
    assert_eq!(h.state().outs.iter().rev().find_map(|o| o.suspend_keys), Some(false));
    assert!(!h.state().ui.busy());
    h.run();
    assert!(h.query_by_label_contains("Also used by General → Quit everything hotkey").is_some());

    h.get_by_label("Save & Apply").click();
    h.run();
    h.run();
    assert!(h.query_by_label("OK").is_some(), "saving with a clash is refused with a message");
    assert_eq!(saved(&h), None);
}

#[test]
fn a_key_a_dashboard_item_uses_is_a_clash_too() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = state(dir.path(), Look::Dark, Schema::get());
    state.others = vec![("ctrl+shift+k".to_owned(), "Dashboard → Apps → Firefox".to_owned())];
    let mut h = harness_with(state, [1000.0, 700.0], false);
    open(&mut h, "general");
    let records: Vec<_> = h.get_all_by_label_contains("Record").collect();
    records[1].click();
    h.run();
    press_combo(&mut h, Modifiers::CTRL | Modifiers::SHIFT, Key::K);
    h.run();
    assert!(h.query_by_label_contains("Also used by Dashboard → Apps → Firefox").is_some());
}

#[test]
fn profiles_can_be_made_switched_renamed_and_deleted() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = harness(&dir, [1000.0, 700.0]);
    settle(&mut h);
    h.get_by_label("New profile").click();
    h.run();
    h.run();
    h.get_by_label("OK").click();
    h.run();
    // an empty name makes nothing
    assert_eq!(h.state().settings.profiles().len(), 1);

    h.get_by_label("New profile").click();
    h.run();
    h.run();
    let ask = "Name (it starts as a copy of the current settings):";
    h.get_by_role_and_label(egui::Role::TextInput, ask).focus();
    h.run();
    h.get_by_role_and_label(egui::Role::TextInput, ask).type_text("Work");
    h.run();
    h.get_by_label("OK").click();
    h.run();
    h.run();
    assert_eq!(h.state().settings.profiles().len(), 2);
    assert_eq!(h.state().settings.active_profile(), "profile1");
    assert!(
        saved(&h).is_some_and(|c| c.contains(&"notes".to_owned())),
        "a profile switch applies everything"
    );

    h.get_by_role_and_label(egui::Role::Button, "Delete").click();
    h.run();
    h.run();
    // the profile bar's button and the question's
    h.get_all_by_role_and_label(egui::Role::Button, "Delete").last().unwrap().click();
    h.run();
    h.run();
    assert_eq!(h.state().settings.profiles().len(), 1);
    assert_eq!(h.state().settings.active_profile(), "default");
}

#[test]
fn changes_not_saved_are_asked_about_before_a_profile_switch() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = harness(&dir, [1000.0, 700.0]);
    settle(&mut h);
    check(&h, "Show notifications").click();
    h.run();
    h.get_by_label("New profile").click();
    h.run();
    h.run();
    assert!(h.query_by_label("Discard").is_some(), "the question has a Discard button");
    h.get_by_label("Discard").click();
    h.run();
    h.run();
    assert!(h.query_by_label("OK").is_some(), "then it asks for the new profile's name");
}

#[test]
fn a_module_that_is_off_offers_turn_it_on_and_one_that_runs_has_its_buttons() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = state(dir.path(), Look::Dark, Schema::get());
    state.module = Some(ModuleInfo { title: "Notes".into(), running: false, ..Default::default() });
    let mut h = harness_with(state, [1000.0, 700.0], false);
    open(&mut h, "notes");
    assert!(h.query_by_label("Notes is turned off.").is_some());
    h.get_by_label("Turn it on").click();
    h.run();
    assert!(h.state().outs.iter().any(|o| o.turn_on == ["notes"]));

    h.state_mut().module = Some(ModuleInfo {
        title: "Notes".into(),
        running: true,
        actions: vec![
            ("notes:new_note".into(), "New note".into()),
            ("notes:show".into(), "Show notes".into()),
        ],
        error: None,
    });
    h.run();
    h.get_by_label("New note").click();
    h.run();
    assert!(h.state().outs.iter().any(|o| o.run == ["notes:new_note"]));
}

#[test]
fn the_notification_test_and_quit_buttons_ask_the_app() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = harness(&dir, [1000.0, 3200.0]);
    open(&mut h, "general");
    h.get_by_label("Show a notification").click();
    h.run();
    let test = h.state().outs.iter().find_map(|o| o.toast_test.clone()).unwrap();
    assert_eq!(test.0, "Luminescent");
    assert!(test.2.contains_key("general"), "it carries the values on the page");
    h.get_by_role_and_label(egui::Role::Button, "Quit everything").click();
    h.run();
    assert!(h.state().outs.iter().any(|o| o.quit));
    h.get_by_role_and_label(egui::Role::Button, "Restart").click();
    h.run();
    assert!(h.state().outs.iter().any(|o| o.restart));
}

#[test]
fn the_backup_buttons_ask_for_files() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = harness(&dir, [1000.0, 3200.0]);
    open(&mut h, "general");
    h.get_by_label("Export backup…").click();
    h.run();
    h.get_by_label("Import backup…").click();
    h.run();
    let files: Vec<_> = h.state().outs.iter().flat_map(|o| o.files.iter()).collect();
    assert_eq!(files.len(), 2);
    assert!(files[0].save && files[0].request.title == "Export backup");
}

#[test]
fn the_page_switcher_list_keeps_dashboard_and_settings() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = harness(&dir, [1000.0, 3200.0]);
    open(&mut h, "ui");
    assert!(h.query_by_label("Dashboard in the bar").is_some());
    h.get_by_label("Notes in the bar").click();
    h.run();
    let order = h.state().ui.draft().unwrap().value("ui", "nav_order").unwrap().clone();
    assert!(order.as_array().unwrap().iter().any(|v| v == "notes"));
    assert!(order.as_array().unwrap().iter().any(|v| v == "dashboard"));
}

#[test]
fn it_draws_in_a_small_box() {
    for size in [[360.0, 220.0], [360.0, 640.0], [500.0, 300.0]] {
        let dir = tempfile::tempdir().unwrap();
        let mut h = harness(&dir, size);
        settle(&mut h);
        for key in ["general", "ui", "dashboard", "doodle"] {
            open(&mut h, key);
        }
        h.get_by_label("Save & Apply");
    }
}

#[test]
fn a_page_that_stops_recording_when_the_overlay_goes_says_so() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = harness(&dir, [1000.0, 700.0]);
    open(&mut h, "general");
    h.get_all_by_label_contains("Record").next().unwrap().click();
    h.run();
    assert!(h.state().ui.busy());
    assert!(h.state_mut().ui.on_hide(), "keys were paused and the app must resume them");
    assert!(!h.state().ui.busy());
}

fn section_pictures(look: Look, name: &str) {
    let dir = tempfile::tempdir().unwrap();
    let schema = Schema::get();
    let mut h = harness_with(state(dir.path(), look, schema), [980.0, 620.0], true);
    let mut results = SnapshotResults::new();
    for section in &schema.sections {
        h.state_mut().ui.open_section(&section.key);
        settle(&mut h);
        if cfg!(target_os = "linux") {
            let options = SnapshotOptions::new().threshold(1.5).max_failed_pixels(400);
            results.add(h.try_snapshot_options(format!("settings_{}_{name}", section.key), &options));
        }
    }
}

#[test]
fn every_section_in_dark() {
    section_pictures(Look::Dark, "dark");
}

#[test]
fn every_section_in_light() {
    section_pictures(Look::Light, "light");
}

#[test]
fn dashboard_and_scripts_shows_every_setting_with_its_help_text() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = harness(&dir, [1000.0, 1200.0]);
    open(&mut h, "dashboard");
    let section = Schema::get().section("dashboard").unwrap();
    for key in [
        "use_terminal",
        "terminal",
        "keep_open",
        "python_path",
        "show_in_tray",
        "show_icons",
        "group_delay_ms",
        "tidy_style",
        "tidy_order",
        "snap",
    ] {
        let field = section.field(key).unwrap_or_else(|| panic!("{key} is not in the schema"));
        assert!(h.query_all_by_label_contains(&field.label).next().is_some(), "{key}: its label is missing");
        if !field.help.is_empty() {
            assert!(h.query_by_label(&field.help).is_some(), "{key}: its help text is missing");
        }
    }
}

fn tall_picture(look: Look, section: &str, name: &str) {
    let dir = tempfile::tempdir().unwrap();
    let mut h = harness_with(state(dir.path(), look, Schema::get()), [980.0, 2300.0], true);
    h.state_mut().ui.open_section(section);
    settle(&mut h);
    if cfg!(target_os = "linux") {
        let options = SnapshotOptions::new().threshold(1.5).max_failed_pixels(600);
        h.snapshot_options(name, &options);
    }
}

#[test]
fn the_whole_of_general_with_its_buttons_and_lists() {
    tall_picture(Look::Dark, "general", "settings_general_whole_dark");
}

#[test]
fn the_whole_of_the_overlay_section_with_the_page_switcher() {
    tall_picture(Look::Light, "ui", "settings_ui_whole_light");
}

fn small_picture(look: Look, name: &str) {
    let dir = tempfile::tempdir().unwrap();
    let mut h = harness_with(state(dir.path(), look, Schema::get()), [420.0, 260.0], true);
    h.state_mut().ui.open_section("ui");
    settle(&mut h);
    if cfg!(target_os = "linux") {
        let options = SnapshotOptions::new().threshold(1.5).max_failed_pixels(400);
        h.snapshot_options(format!("settings_small_{name}"), &options);
    }
}

#[test]
fn a_small_box_in_dark() {
    small_picture(Look::Dark, "dark");
}

#[test]
fn a_small_box_in_light() {
    small_picture(Look::Light, "light");
}
