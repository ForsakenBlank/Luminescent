//! Snippets: pictures of the page, the Ctrl+B pop-up and a small box in Dark and Light, and what clicking,
//! searching, dragging and the right-click menu do to the snippets folder and its `.luminescent.json`.
//! Pictures are compared on Linux only.

#![allow(clippy::unwrap_used)]

use std::path::{Path, PathBuf};
use std::time::Duration;

use egui::{Modifiers, Pos2};
use egui_kittest::kittest::Queryable;
use egui_kittest::{Harness, SnapshotOptions};
use lum_core::Paths;
use lum_core::events::EventBus;
use lum_core::module::{LogNotifier, Module, ModuleCx};
use lum_core::settings::Settings;
use lum_ui::tools::snippets::SnippetsModule;
use lum_ui::{Request, Services, Tokens, UiCx, UiModule};
use serde_json::{Value, json};

const META: &str = include_str!("../../../fixtures/sample_home/snippets/.luminescent.json");

#[derive(Clone, Copy, PartialEq, Eq)]
enum Show {
    Page,
    Popup,
}

struct State {
    module: SnippetsModule,
    settings: Settings,
    services: Services,
    light: bool,
    ready: bool,
    show: Show,
}

fn write(dir: &Path, name: &str, text: &str) {
    let path = dir.join(name);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

/// A folder of clips with one of each kind of card, a folder with a clip in it and an empty folder.
fn snippets_folder(dir: &Path) -> PathBuf {
    let base = dir.join("Clipboard");
    write(&base, ".luminescent.json", META);
    write(&base, "hello.txt", "Hello {clipboard}, sent {date}.");
    write(&base, "meeting.txt", "Meeting notes\nbring the laptop\nand the charger");
    write(&base, "Docs.txt", "https://example.com/docs/start");
    write(&base, "blue.txt", "#3E63DD");
    write(&base, "tool.py", "def hello():\n    print('hi')\n");
    write(&base, "archive.zip", "not really a zip");
    write(&base, "Work/invoice.txt", "Invoice 42 is due on Friday");
    write(&base, "Work/Sub/deep.txt", "far down");
    std::fs::create_dir_all(base.join("Archive")).unwrap();
    let picture =
        image::RgbaImage::from_fn(200, 120, |x, y| image::Rgba([(x + 20) as u8, (y * 2) as u8, 160, 255]));
    picture.save(base.join("photo.png")).unwrap();
    base
}

fn make(dir: &Path, light: bool, show: Show, base: &Path) -> State {
    let paths = Paths::custom(dir);
    paths.create_dirs().unwrap();
    let mut settings = Settings::load(&paths);
    settings.set("snippets.base_path", json!(base.to_string_lossy()));
    let services = Services::detached(paths.data.clone());
    let mut module = SnippetsModule::new(&services);
    let events = EventBus::new();
    let mut cx =
        ModuleCx { settings: &mut settings, events: &events, notifier: &LogNotifier, safe_mode: false };
    module.start(&mut cx).unwrap();
    State { module, settings, services, light, ready: false, show }
}

fn harness(state: State, size: [f32; 2], render: bool) -> Harness<'static, State> {
    let builder = Harness::builder().with_size(size).with_pixels_per_point(1.0);
    let builder = if render && cfg!(target_os = "linux") { builder.wgpu() } else { builder };
    builder.build_ui_state(
        |ui, state: &mut State| {
            if !state.ready {
                lum_ui::fonts::install(ui.ctx());
                lum_ui::Motion::off().install(ui.ctx());
                if state.light { Tokens::light() } else { Tokens::dark() }.install(ui.ctx());
                state.ready = true;
            }
            let ctx = ui.ctx().clone();
            let mut cx = UiCx::new(&ctx, &state.settings);
            if state.show == Show::Page {
                let rect = ui.max_rect();
                ui.painter().rect_filled(rect, 0.0, Tokens::of(&ctx).p.bg_base);
                state.module.page(ui, rect, &mut cx);
            }
            state.module.show(&mut cx);
        },
        state,
    )
}

/// Frames until the folder has been listed on its thread and the thumbnails are in.
fn settle(h: &mut Harness<'_, State>) {
    for _ in 0..60 {
        h.step();
        std::thread::sleep(Duration::from_millis(15));
    }
    h.run();
}

fn photograph(h: &mut Harness<'_, State>, name: &str) {
    settle(h);
    if cfg!(target_os = "linux") {
        let options = SnapshotOptions::new().threshold(1.5).max_failed_pixels(600);
        h.snapshot_options(name, &options);
    }
}

fn finish(mut h: Harness<'static, State>) {
    let state = h.state_mut();
    let mut cx = ModuleCx {
        settings: &mut state.settings,
        events: &EventBus::new(),
        notifier: &LogNotifier,
        safe_mode: false,
    };
    state.module.stop(&mut cx);
}

fn meta(base: &Path) -> Value {
    serde_json::from_str(&std::fs::read_to_string(base.join(".luminescent.json")).unwrap()).unwrap()
}

fn press(h: &mut Harness<'_, State>, at: Pos2) {
    h.hover_at(at);
    h.event(egui::Event::PointerButton {
        pos: at,
        button: egui::PointerButton::Primary,
        pressed: true,
        modifiers: Modifiers::NONE,
    });
}

fn release(h: &mut Harness<'_, State>, at: Pos2) {
    h.event(egui::Event::PointerButton {
        pos: at,
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: Modifiers::NONE,
    });
}

/// Drags in steps, a frame between each like a real mouse. A drag asks for a frame after every move, so the
/// harness never settles during one: `step`, not `run`.
fn drag(h: &mut Harness<'_, State>, from: Pos2, to: Pos2) {
    press(h, from);
    h.step();
    for step in 1..=6 {
        h.hover_at(from + (to - from) * (step as f32 / 6.0));
        h.step();
    }
    release(h, to);
    h.step();
    settle(h);
}

const PAGE: [f32; 2] = [940.0, 640.0];
const POPUP: [f32; 2] = [1000.0, 620.0];

#[test]
fn page_dark() {
    let dir = tempfile::tempdir().unwrap();
    let base = snippets_folder(dir.path());
    photograph(&mut harness(make(dir.path(), false, Show::Page, &base), PAGE, true), "snippets_page_dark");
}

#[test]
fn page_light() {
    let dir = tempfile::tempdir().unwrap();
    let base = snippets_folder(dir.path());
    photograph(&mut harness(make(dir.path(), true, Show::Page, &base), PAGE, true), "snippets_page_light");
}

#[test]
fn page_as_a_small_box_dark() {
    let dir = tempfile::tempdir().unwrap();
    let base = snippets_folder(dir.path());
    photograph(
        &mut harness(make(dir.path(), false, Show::Page, &base), [420.0, 560.0], true),
        "snippets_box_dark",
    );
}

#[test]
fn popup_dark() {
    let dir = tempfile::tempdir().unwrap();
    let base = snippets_folder(dir.path());
    let mut h = harness(make(dir.path(), false, Show::Popup, &base), POPUP, true);
    h.run();
    h.state_mut().module.bind_pressed("open");
    // in the app the dim is a window of its own under the pop-up; in the harness it would be drawn over it
    h.state_mut().settings.set("snippets.overlay_dim", json!(0.0));
    photograph(&mut h, "snippets_popup_dark");
}

#[test]
fn popup_light() {
    let dir = tempfile::tempdir().unwrap();
    let base = snippets_folder(dir.path());
    let mut h = harness(make(dir.path(), true, Show::Popup, &base), POPUP, true);
    h.run();
    h.state_mut().module.bind_pressed("open");
    h.state_mut().settings.set("snippets.overlay_dim", json!(0.0));
    photograph(&mut h, "snippets_popup_light");
}

#[test]
fn resting_the_mouse_on_a_card_shows_the_bigger_look() {
    let dir = tempfile::tempdir().unwrap();
    let base = snippets_folder(dir.path());
    let mut h = harness(make(dir.path(), false, Show::Popup, &base), POPUP, true);
    h.run();
    h.state_mut().module.bind_pressed("open");
    h.state_mut().settings.set("snippets.overlay_dim", json!(0.0));
    settle(&mut h);
    let card = h.get_by_label("Clip Docs.txt").rect();
    h.hover_at(card.center());
    for _ in 0..4 {
        h.step();
    }
    photograph(&mut h, "snippets_hover_dark");
}

#[test]
fn a_folder_that_is_missing_offers_to_make_it() {
    let dir = tempfile::tempdir().unwrap();
    let base = dir.path().join("not-yet");
    let mut h = harness(make(dir.path(), false, Show::Page, &base), PAGE, true);
    photograph(&mut h, "snippets_no_folder_dark");
    h.get_by_label("Make this folder").click();
    settle(&mut h);
    assert!(base.is_dir());
    assert!(h.query_by_label("Make this folder").is_none());
    finish(h);
}

#[test]
fn the_grid_shows_favourites_folders_and_clips() {
    let dir = tempfile::tempdir().unwrap();
    let base = snippets_folder(dir.path());
    let mut h = harness(make(dir.path(), false, Show::Page, &base), PAGE, false);
    settle(&mut h);
    for label in [
        "Folder Work",
        "Folder Archive",
        "Clip meeting.txt",
        "Clip blue.txt",
        "Clip photo.png",
        "Clip tool.py",
    ] {
        assert!(h.query_by_label(label).is_some(), "{label} is missing");
    }
    assert_eq!(
        h.query_all_by_label("Clip hello.txt").count(),
        2,
        "once as a favourite and once in the folder"
    );
    assert!(h.query_by_label("Clip invoice.txt").is_none(), "nothing from inside a folder");
    finish(h);
}

#[test]
fn opening_a_folder_shows_its_clips_and_backspace_goes_back_up() {
    let dir = tempfile::tempdir().unwrap();
    let base = snippets_folder(dir.path());
    let mut h = harness(make(dir.path(), false, Show::Page, &base), PAGE, false);
    settle(&mut h);
    h.get_by_label("Folder Work").click();
    settle(&mut h);
    assert!(h.query_by_label("Clip invoice.txt").is_some());
    assert!(h.query_by_label("Folder Sub").is_some());
    assert!(h.query_by_label("Clip meeting.txt").is_none());
    assert!(h.query_by_label("Folder Clipboard").is_some(), "the breadcrumb can go back to the top");
    h.key_press(egui::Key::Backspace);
    settle(&mut h);
    assert!(h.query_by_label("Clip meeting.txt").is_some());
    h.get_by_label("Folder Work").click();
    settle(&mut h);
    h.get_by_label("Folder Clipboard").click();
    settle(&mut h);
    assert!(h.query_by_label("Clip meeting.txt").is_some());
    finish(h);
}

#[test]
fn search_looks_in_every_folder() {
    let dir = tempfile::tempdir().unwrap();
    let base = snippets_folder(dir.path());
    let mut h = harness(make(dir.path(), false, Show::Page, &base), PAGE, false);
    settle(&mut h);
    let search = h.get_by_role_and_label(egui::Role::TextInput, "Search every folder");
    search.focus();
    h.run();
    h.get_by_role_and_label(egui::Role::TextInput, "Search every folder").type_text("invoice");
    settle(&mut h);
    assert!(h.query_by_label("Clip invoice.txt").is_some(), "found by name, in a folder");
    assert!(h.query_by_label("Clip meeting.txt").is_none());
    h.get_by_role_and_label(egui::Role::TextInput, "Search every folder").type_text("zzz");
    settle(&mut h);
    assert!(h.query_by_label("Clip invoice.txt").is_none());
    finish(h);
}

#[test]
fn search_finds_what_a_text_clip_says() {
    let dir = tempfile::tempdir().unwrap();
    let base = snippets_folder(dir.path());
    let mut h = harness(make(dir.path(), false, Show::Page, &base), PAGE, false);
    settle(&mut h);
    h.get_by_role_and_label(egui::Role::TextInput, "Search every folder").focus();
    h.run();
    h.get_by_role_and_label(egui::Role::TextInput, "Search every folder").type_text("far down");
    settle(&mut h);
    assert!(h.query_by_label("Clip deep.txt").is_some());
    finish(h);
}

#[test]
fn clicking_a_clip_on_the_page_closes_the_overlay() {
    let dir = tempfile::tempdir().unwrap();
    let base = snippets_folder(dir.path());
    let mut h = harness(make(dir.path(), false, Show::Page, &base), PAGE, false);
    settle(&mut h);
    h.get_by_label("Clip meeting.txt").click();
    h.run();
    let requests = h.state().services.take();
    assert!(requests.contains(&Request::HideOverlay), "{requests:?}");
    finish(h);
}

#[test]
fn clicking_a_clip_in_the_popup_closes_it() {
    let dir = tempfile::tempdir().unwrap();
    let base = snippets_folder(dir.path());
    let mut h = harness(make(dir.path(), false, Show::Popup, &base), POPUP, false);
    h.run();
    h.state_mut().module.bind_pressed("open");
    settle(&mut h);
    assert!(h.query_by_label("Clip blue.txt").is_some());
    h.get_by_label("Clip blue.txt").click();
    settle(&mut h);
    assert!(h.query_by_label("Clip blue.txt").is_none(), "the pop-up is gone");
    finish(h);
}

#[test]
fn the_hotkey_closes_the_popup_again() {
    let dir = tempfile::tempdir().unwrap();
    let base = snippets_folder(dir.path());
    let mut h = harness(make(dir.path(), false, Show::Popup, &base), POPUP, false);
    h.run();
    h.state_mut().module.bind_pressed("open");
    settle(&mut h);
    assert!(h.query_by_label("Clip meeting.txt").is_some());
    h.state_mut().module.bind_pressed("open");
    settle(&mut h);
    assert!(h.query_by_label("Clip meeting.txt").is_none());
    finish(h);
}

#[test]
fn dragging_a_clip_onto_a_folder_moves_it_there() {
    let dir = tempfile::tempdir().unwrap();
    let base = snippets_folder(dir.path());
    let mut h = harness(make(dir.path(), false, Show::Page, &base), PAGE, false);
    settle(&mut h);
    let from = h.get_by_label("Clip meeting.txt").rect().center();
    let to = h.get_by_label("Folder Archive").rect().center();
    drag(&mut h, from, to);
    assert!(base.join("Archive/meeting.txt").exists());
    assert!(!base.join("meeting.txt").exists());
    assert!(h.query_by_label("Clip meeting.txt").is_none());
    finish(h);
}

#[test]
fn dragging_a_clip_onto_another_clip_puts_it_before_that_clip() {
    let dir = tempfile::tempdir().unwrap();
    let base = snippets_folder(dir.path());
    let mut h = harness(make(dir.path(), false, Show::Page, &base), PAGE, false);
    settle(&mut h);
    let from = h.get_by_label("Clip tool.py").rect().center();
    // the card in the folder's own grid, not the favourites row above it
    let target = h
        .get_all_by_label("Clip hello.txt")
        .map(|n| n.rect())
        .max_by(|a, b| a.min.y.total_cmp(&b.min.y))
        .unwrap();
    drag(&mut h, from, target.center());
    finish(h);
    let order = meta(&base)["order"][""].clone();
    let names: Vec<&str> = order.as_array().unwrap().iter().filter_map(Value::as_str).collect();
    let at = |n: &str| names.iter().position(|x| *x == n);
    assert!(at("tool.py") < at("hello.txt"), "{names:?}");
    assert!(at("tool.py").is_some());
}

#[test]
fn the_right_click_menu_makes_a_favourite_and_takes_it_back() {
    let dir = tempfile::tempdir().unwrap();
    let base = snippets_folder(dir.path());
    let mut h = harness(make(dir.path(), false, Show::Page, &base), PAGE, false);
    settle(&mut h);
    h.get_by_label("Clip meeting.txt").click_secondary();
    h.run();
    h.get_by_label("★  Favourite (row at the top)").click();
    settle(&mut h);
    assert_eq!(h.query_all_by_label("Clip meeting.txt").count(), 2, "it is in the favourites row now");
    let target = h
        .get_all_by_label("Clip meeting.txt")
        .map(|n| n.rect())
        .min_by(|a, b| a.min.y.total_cmp(&b.min.y))
        .unwrap();
    h.hover_at(target.center());
    h.event(egui::Event::PointerButton {
        pos: target.center(),
        button: egui::PointerButton::Secondary,
        pressed: true,
        modifiers: Modifiers::NONE,
    });
    h.event(egui::Event::PointerButton {
        pos: target.center(),
        button: egui::PointerButton::Secondary,
        pressed: false,
        modifiers: Modifiers::NONE,
    });
    h.run();
    h.get_by_label("☆  Remove from favourites").click();
    settle(&mut h);
    assert_eq!(h.query_all_by_label("Clip meeting.txt").count(), 1);
    finish(h);
    let favourites = meta(&base)["favourites"].clone();
    assert_eq!(favourites, json!(["hello.txt"]));
}

#[test]
fn the_kept_file_survives_a_favourite_change_with_its_other_keys() {
    let dir = tempfile::tempdir().unwrap();
    let base = snippets_folder(dir.path());
    let mut h = harness(make(dir.path(), false, Show::Page, &base), PAGE, false);
    settle(&mut h);
    h.get_by_label("Clip blue.txt").click_secondary();
    h.run();
    h.get_by_label("★  Favourite (row at the top)").click();
    settle(&mut h);
    finish(h);
    let saved = meta(&base);
    assert_eq!(saved["folder_colours"], json!({"Work": "#3E63DD"}));
    assert_eq!(saved["favourites"], json!(["hello.txt", "blue.txt"]));
    assert_eq!(saved["order"][""], json!(["Work", "hello.txt"]), "a key written by hand keeps its name");
}

#[test]
fn a_changed_folder_in_settings_starts_the_page_over() {
    let dir = tempfile::tempdir().unwrap();
    let base = snippets_folder(dir.path());
    let other = dir.path().join("Other");
    write(&other, "elsewhere.txt", "hi");
    let mut h = harness(make(dir.path(), false, Show::Page, &base), PAGE, false);
    settle(&mut h);
    assert!(h.query_by_label("Clip meeting.txt").is_some());
    h.state_mut().settings.set("snippets.base_path", json!(other.to_string_lossy()));
    settle(&mut h);
    assert!(h.query_by_label("Clip elsewhere.txt").is_some());
    assert!(h.query_by_label("Clip meeting.txt").is_none());
    finish(h);
}

#[test]
fn a_page_that_comes_back_lists_the_folder_again() {
    let dir = tempfile::tempdir().unwrap();
    let base = snippets_folder(dir.path());
    let mut h = harness(make(dir.path(), false, Show::Page, &base), PAGE, false);
    settle(&mut h);
    write(&base, "fresh.txt", "made while the page was away");
    h.state_mut().show = Show::Popup;
    settle(&mut h);
    h.state_mut().show = Show::Page;
    settle(&mut h);
    assert!(h.query_by_label("Clip fresh.txt").is_some());
    finish(h);
}
