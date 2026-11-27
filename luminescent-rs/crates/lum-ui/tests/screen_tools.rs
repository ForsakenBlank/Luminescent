//! The colour picker, the ruler and text grab on two monitors with different scales (DP-2 is at 2x), grabbed
//! by a stand-in for the system's screenshots and drawn side by side in one test window: pictures in Dark and
//! Light, what a click copies, the keys, and the colour dialog borrowing the picker. Pictures are compared on
//! Linux only.

#![allow(clippy::unwrap_used)]

use std::sync::Arc;
use std::time::Duration;

use egui::{Rect, pos2, vec2};
use egui_kittest::kittest::Queryable;
use egui_kittest::{Harness, SnapshotOptions};
use lum_core::Paths;
use lum_core::events::EventBus;
use lum_core::module::{LogNotifier, ModuleCx};
use lum_core::settings::Settings;
use lum_platform::capture::Fake;
use lum_ui::window::{self, Screen};
use lum_ui::{Request, Services, Tokens, UiCx, UiModule};

const W: f32 = 640.0;
const H: f32 = 400.0;

fn two_screens() -> Vec<Screen> {
    vec![
        Screen::new("DP-1", Rect::from_min_size(pos2(0.0, 0.0), vec2(W, H))),
        Screen::new("DP-2", Rect::from_min_size(pos2(W, 0.0), vec2(W, H))).with_scale(2.0),
    ]
}

/// Real pixels per layout pixel on DP-2 (Windows lists monitors in real pixels already).
fn k() -> u32 {
    if cfg!(windows) { 1 } else { 2 }
}

struct State<M> {
    module: M,
    settings: Settings,
    services: Services,
    light: bool,
    ready: bool,
}

fn make<M: UiModule>(
    dir: &std::path::Path,
    light: bool,
    new: impl Fn(&Services) -> M,
    set: &[(&str, serde_json::Value)],
) -> State<M> {
    let paths = Paths::custom(dir);
    paths.create_dirs().unwrap();
    let mut settings = Settings::load(&paths);
    for (key, value) in set {
        settings.set(key, value.clone());
    }
    let services = Services::detached(paths.data.clone());
    let mut module = new(&services);
    let events = EventBus::new();
    let mut cx =
        ModuleCx { settings: &mut settings, events: &events, notifier: &LogNotifier, safe_mode: false };
    module.start(&mut cx).unwrap();
    State { module, settings, services, light, ready: false }
}

fn harness<M: UiModule + 'static>(state: State<M>, render: bool) -> Harness<'static, State<M>> {
    let builder = Harness::builder().with_size([W * 2.0, H]).with_pixels_per_point(1.0);
    let builder = if render && cfg!(target_os = "linux") { builder.wgpu() } else { builder };
    builder.build_ui_state(
        |ui, state: &mut State<M>| {
            if !state.ready {
                lum_ui::fonts::install(ui.ctx());
                lum_ui::Motion::off().install(ui.ctx());
                if state.light { Tokens::light() } else { Tokens::dark() }.install(ui.ctx());
                window::install_capture(ui.ctx(), Arc::new(Fake::default()));
                window::install_screens(ui.ctx(), two_screens());
                state.ready = true;
            }
            let ctx = ui.ctx().clone();
            let mut cx = UiCx::new(&ctx, &state.settings);
            state.module.show(&mut cx);
        },
        state,
    )
}

/// Frames until the screens have been grabbed on their thread.
fn settle<M: UiModule>(h: &mut Harness<'_, State<M>>) {
    for _ in 0..20 {
        h.step();
        std::thread::sleep(Duration::from_millis(10));
    }
    h.run();
}

fn photograph<M: UiModule>(h: &mut Harness<'_, State<M>>, name: &str) {
    h.run();
    if cfg!(target_os = "linux") {
        let options = SnapshotOptions::new().threshold(1.5).max_failed_pixels(600);
        h.snapshot_options(name, &options);
    }
}

fn notes<M>(state: &State<M>) -> Vec<String> {
    state
        .services
        .take()
        .into_iter()
        .filter_map(|r| match r {
            Request::Notify { text, .. } => Some(text),
            _ => None,
        })
        .collect()
}

mod colour_picker {
    use super::*;
    use lum_ui::tools::colorpicker::{ColorPickerModule, eyedropper};

    fn opened(
        dir: &std::path::Path,
        light: bool,
        render: bool,
        set: &[(&str, serde_json::Value)],
    ) -> Harness<'static, State<ColorPickerModule>> {
        let mut h = harness(make(dir, light, ColorPickerModule::new, set), render);
        h.run();
        h.state_mut().module.bind_pressed("pick");
        settle(&mut h);
        assert!(h.state().module.is_picking());
        h
    }

    /// The fake's colour at a layout point of DP-2, as `#RRGGBB`.
    fn on_dp2(x: u32, y: u32) -> String {
        let [r, g, b] = Fake::colour_at(1, x * k(), y * k());
        format!("#{r:02X}{g:02X}{b:02X}")
    }

    #[test]
    fn the_loupe_follows_the_mouse_dark() {
        let dir = tempfile::tempdir().unwrap();
        let mut h = opened(dir.path(), false, true, &[]);
        h.hover_at(pos2(W + 300.0, 150.0));
        h.run();
        assert!(h.query_by_label(&format!("Loupe {}", on_dp2(300, 150))).is_some());
        photograph(&mut h, "colour_picker_dark");
    }

    #[test]
    fn the_loupe_follows_the_mouse_light() {
        let dir = tempfile::tempdir().unwrap();
        let mut h = opened(dir.path(), true, true, &[]);
        // near the bottom right corner the card goes up and to the left
        h.hover_at(pos2(W + W - 20.0, H - 20.0));
        h.run();
        photograph(&mut h, "colour_picker_light");
    }

    #[test]
    fn a_click_copies_the_real_pixel_and_remembers_it() {
        let dir = tempfile::tempdir().unwrap();
        let mut h = opened(dir.path(), false, false, &[]);
        h.hover_at(pos2(W + 100.0, 200.0));
        h.run();
        h.drag_at(pos2(W + 100.0, 200.0));
        h.run();
        let want = on_dp2(100, 200);
        assert!(!h.state().module.is_picking());
        assert_eq!(h.state().module.recent(), std::slice::from_ref(&want));
        assert_eq!(notes(h.state()), [format!("{want} copied")]);
        let saved = std::fs::read_to_string(dir.path().join("data/colours.json")).unwrap();
        assert_eq!(saved, format!(r#"{{"recent": ["{want}"]}}"#));
        // the tray lists it, with the theme choices
        let tray = h.state().module.tray_menu();
        assert!(
            matches!(&tray[0], lum_platform::tray::MenuItem::Sub { text, .. } if text == "🎯  Recent colours")
        );
    }

    #[test]
    fn arrows_nudge_and_enter_picks_in_the_chosen_format() {
        let dir = tempfile::tempdir().unwrap();
        let set = [("colorpicker.format", serde_json::json!("rgb"))];
        let mut h = opened(dir.path(), false, false, &set);
        h.hover_at(pos2(40.0, 40.0));
        h.run();
        h.key_press(egui::Key::ArrowRight);
        h.run();
        h.key_press_modifiers(egui::Modifiers::SHIFT, egui::Key::ArrowDown);
        h.run();
        h.key_press(egui::Key::Enter);
        h.run();
        let [r, g, b] = Fake::colour_at(0, 41, 50);
        assert_eq!(notes(h.state()), [format!("rgb({r}, {g}, {b}) copied")]);
    }

    #[test]
    fn escape_and_a_right_click_cancel() {
        let dir = tempfile::tempdir().unwrap();
        let mut h = opened(dir.path(), false, false, &[]);
        h.hover_at(pos2(40.0, 40.0));
        h.run();
        h.key_press(egui::Key::Escape);
        h.run();
        assert!(!h.state().module.is_picking());
        h.state_mut().module.bind_pressed("pick");
        settle(&mut h);
        h.event(egui::Event::PointerButton {
            pos: pos2(50.0, 50.0),
            button: egui::PointerButton::Secondary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        });
        h.run();
        assert!(!h.state().module.is_picking());
        assert!(h.state().module.recent().is_empty());
        assert!(notes(h.state()).is_empty());
    }

    #[test]
    fn the_colour_dialog_borrows_the_picker() {
        let dir = tempfile::tempdir().unwrap();
        let mut h = harness(make(dir.path(), false, ColorPickerModule::new, &[]), false);
        h.run();
        let who = egui::Id::unique("a test dialog");
        assert!(eyedropper::available(&h.ctx));
        eyedropper::ask(&h.ctx, who);
        settle(&mut h);
        assert!(h.state().module.is_picking());
        h.hover_at(pos2(W + 10.0, 10.0));
        h.run();
        h.drag_at(pos2(W + 10.0, 10.0));
        h.run();
        let got = eyedropper::take_answer(&h.ctx, who).unwrap().unwrap();
        assert_eq!(got.hex(), on_dp2(10, 10));
        // nothing was copied: the colour went to the dialog
        assert!(notes(h.state()).is_empty());
        assert_eq!(h.state().module.recent(), [on_dp2(10, 10)]);
    }

    #[test]
    fn recent_colours_are_palette_rows_that_reach_the_theme() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("data")).unwrap();
        std::fs::write(dir.path().join("data/colours.json"), r##"{"recent": ["#7B6CF6"]}"##).unwrap();
        let mut state = make(dir.path(), false, ColorPickerModule::new, &[]);
        let rows: Vec<_> = state.module.palette_entries().into_iter().map(|e| (e.id, e.label)).collect();
        assert_eq!(
            rows,
            [
                ("colour:#7B6CF6".to_owned(), "Colour #7B6CF6".to_owned()),
                ("colour-accent:#7B6CF6".to_owned(), "Accent colour #7B6CF6".to_owned()),
                ("colour-theme:#7B6CF6".to_owned(), "New theme from #7B6CF6".to_owned()),
            ]
        );
        let services = state.services.clone();
        assert!(state.module.run_palette("colour-accent:#7B6CF6", &services));
        assert!(state.module.run_palette("colour-light:#7B6CF6", &services));
        assert!(!state.module.run_palette("note:1", &services));
        assert_eq!(
            services.take(),
            [
                Request::ThemeAccent("#7B6CF6".into()),
                Request::ThemeFromColour { colour: "#7B6CF6".into(), light: true },
            ]
        );
    }
}

mod ruler {
    use super::*;
    use lum_ui::tools::ruler::RulerModule;

    fn opened(dir: &std::path::Path, light: bool, render: bool) -> Harness<'static, State<RulerModule>> {
        let mut h = harness(make(dir, light, RulerModule::new, &[]), render);
        h.run();
        h.state_mut().module.bind_pressed("measure");
        settle(&mut h);
        assert!(h.state().module.is_measuring());
        h
    }

    fn drag(
        h: &mut Harness<'_, State<RulerModule>>,
        from: egui::Pos2,
        to: egui::Pos2,
        modifiers: egui::Modifiers,
    ) {
        h.hover_at(from);
        h.run();
        h.event(egui::Event::PointerButton {
            pos: from,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers,
        });
        h.run();
        h.event(egui::Event::ModifiersChanged(modifiers));
        h.hover_at(to);
        h.run();
        h.event(egui::Event::PointerButton {
            pos: to,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers,
        });
        h.run();
        h.event(egui::Event::ModifiersChanged(egui::Modifiers::NONE));
        h.run();
    }

    #[test]
    fn crosshairs_before_a_drag_dark() {
        let dir = tempfile::tempdir().unwrap();
        let mut h = opened(dir.path(), false, true);
        h.hover_at(pos2(200.0, 220.0));
        h.run();
        photograph(&mut h, "ruler_crosshairs_dark");
    }

    #[test]
    fn a_drag_measures_in_real_pixels_and_copies_light() {
        let dir = tempfile::tempdir().unwrap();
        let mut h = opened(dir.path(), true, true);
        // on DP-2, at 2x: 200 x 100 layout pixels are 400 x 200 real ones
        drag(&mut h, pos2(W + 100.0, 120.0), pos2(W + 300.0, 220.0), egui::Modifiers::NONE);
        let (w, hh) = (200 * k(), 100 * k());
        assert_eq!(notes(h.state()), [format!("{w} × {hh} px copied")]);
        let distance = f64::from(w).hypot(f64::from(hh));
        assert!(h.query_by_label(&format!("{w} × {hh} px   ·   {distance:.1} px")).is_some());
        // still open after copying, as in Python; Enter copies again and closes
        assert!(h.state().module.is_measuring());
        photograph(&mut h, "ruler_measured_light");
        h.key_press(egui::Key::Enter);
        h.run();
        assert!(!h.state().module.is_measuring());
    }

    #[test]
    fn crosshairs_light() {
        let dir = tempfile::tempdir().unwrap();
        let mut h = opened(dir.path(), true, true);
        h.hover_at(pos2(W + 200.0, 220.0));
        h.run();
        photograph(&mut h, "ruler_crosshairs_light");
    }

    #[test]
    fn a_measure_backwards_dark() {
        let dir = tempfile::tempdir().unwrap();
        let mut h = opened(dir.path(), false, true);
        drag(&mut h, pos2(400.0, 300.0), pos2(150.0, 100.0), egui::Modifiers::NONE);
        photograph(&mut h, "ruler_measured_dark");
    }

    #[test]
    fn shift_keeps_it_level_and_ctrl_copies_the_distance() {
        let dir = tempfile::tempdir().unwrap();
        let mut h = opened(dir.path(), false, false);
        drag(&mut h, pos2(50.0, 50.0), pos2(170.0, 60.0), egui::Modifiers::SHIFT);
        assert_eq!(notes(h.state()), ["120 × 0 px copied"]);
        drag(&mut h, pos2(50.0, 50.0), pos2(80.0, 90.0), egui::Modifiers::CTRL);
        assert_eq!(notes(h.state()), ["50 px copied"]);
    }

    #[test]
    fn a_click_without_a_drag_copies_nothing_and_escape_closes() {
        let dir = tempfile::tempdir().unwrap();
        let mut h = opened(dir.path(), false, false);
        drag(&mut h, pos2(50.0, 50.0), pos2(50.0, 50.0), egui::Modifiers::NONE);
        assert!(notes(h.state()).is_empty());
        h.key_press(egui::Key::Escape);
        h.run();
        assert!(!h.state().module.is_measuring());
    }
}

mod text_grab {
    use super::*;
    use lum_ui::tools::textgrab::TextGrabModule;
    use std::sync::Mutex;

    /// The size of the picture the recogniser was given, and what it answers.
    type Seen = Arc<Mutex<Vec<(u32, u32)>>>;

    fn opened(
        dir: &std::path::Path,
        light: bool,
        render: bool,
        set: &[(&str, serde_json::Value)],
        answer: Result<&'static str, &'static str>,
    ) -> (Harness<'static, State<TextGrabModule>>, Seen) {
        let seen: Seen = Arc::default();
        let record = seen.clone();
        let mut state = make(dir, light, TextGrabModule::new, set);
        state.module.set_recogniser(Arc::new(move |frame, _keep| {
            record.lock().unwrap().push((frame.width, frame.height));
            answer.map(str::to_owned).map_err(str::to_owned)
        }));
        let mut h = harness(state, render);
        h.run();
        h.state_mut().module.bind_pressed("grab");
        settle(&mut h);
        assert!(h.state().module.is_selecting());
        (h, seen)
    }

    fn press_and_move(h: &mut Harness<'_, State<TextGrabModule>>, from: egui::Pos2, to: egui::Pos2) {
        h.hover_at(from);
        h.run();
        h.event(egui::Event::PointerButton {
            pos: from,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        });
        h.run();
        h.hover_at(to);
        h.run();
    }

    fn release(h: &mut Harness<'_, State<TextGrabModule>>, at: egui::Pos2) {
        h.event(egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        });
        h.run();
        settle(h);
    }

    #[test]
    fn the_screens_are_dimmed_until_a_box_is_dragged_dark() {
        let dir = tempfile::tempdir().unwrap();
        let (mut h, _) = opened(dir.path(), false, true, &[], Ok("hi"));
        assert_eq!(h.query_all_by_label("Drag a box around the text  ·  Esc to cancel").count(), 2);
        press_and_move(&mut h, pos2(W + 100.0, 100.0), pos2(W + 400.0, 220.0));
        photograph(&mut h, "text_grab_selecting_dark");
    }

    #[test]
    fn the_screens_are_dimmed_until_a_box_is_dragged_light() {
        let dir = tempfile::tempdir().unwrap();
        let (mut h, _) = opened(dir.path(), true, true, &[], Ok("hi"));
        press_and_move(&mut h, pos2(80.0, 120.0), pos2(500.0, 260.0));
        photograph(&mut h, "text_grab_selecting_light");
    }

    #[test]
    fn a_box_is_read_at_real_pixels_and_copied() {
        let dir = tempfile::tempdir().unwrap();
        let set = [("textgrab.upscale", serde_json::json!(false))];
        let (mut h, seen) = opened(dir.path(), false, false, &set, Ok("Hello there\nsecond line"));
        press_and_move(&mut h, pos2(W + 100.0, 100.0), pos2(W + 199.0, 149.0));
        release(&mut h, pos2(W + 199.0, 149.0));
        assert!(!h.state().module.is_selecting());
        // 100 x 50 layout pixels on the 2x screen
        assert_eq!(*seen.lock().unwrap(), [(100 * k(), 50 * k())]);
        assert_eq!(notes(h.state()), ["Copied: Hello there second line"]);
    }

    #[test]
    fn small_boxes_are_enlarged_before_reading() {
        let dir = tempfile::tempdir().unwrap();
        let (mut h, seen) = opened(dir.path(), false, false, &[], Ok("x"));
        press_and_move(&mut h, pos2(10.0, 10.0), pos2(109.0, 29.0));
        release(&mut h, pos2(109.0, 29.0));
        assert_eq!(*seen.lock().unwrap(), [(300, 60)]);
    }

    #[test]
    fn a_slip_of_the_mouse_reads_nothing_and_escape_cancels() {
        let dir = tempfile::tempdir().unwrap();
        let (mut h, seen) = opened(dir.path(), false, false, &[], Ok("x"));
        press_and_move(&mut h, pos2(10.0, 10.0), pos2(13.0, 40.0));
        release(&mut h, pos2(13.0, 40.0));
        assert!(seen.lock().unwrap().is_empty());
        assert!(h.state().module.is_selecting());
        h.key_press(egui::Key::Escape);
        h.run();
        assert!(!h.state().module.is_selecting());
    }

    #[test]
    fn nothing_found_and_errors_are_said() {
        let dir = tempfile::tempdir().unwrap();
        let (mut h, _) = opened(dir.path(), false, false, &[], Ok("  \n "));
        press_and_move(&mut h, pos2(10.0, 10.0), pos2(100.0, 100.0));
        release(&mut h, pos2(100.0, 100.0));
        assert_eq!(notes(h.state()), ["No text found in that area."]);

        let (mut h, _) = opened(dir.path(), false, false, &[], Err("Tesseract is not installed."));
        press_and_move(&mut h, pos2(10.0, 10.0), pos2(100.0, 100.0));
        release(&mut h, pos2(100.0, 100.0));
        let toasts: Vec<_> =
            h.state().services.take().into_iter().filter(|r| matches!(r, Request::Toast { .. })).collect();
        assert_eq!(
            toasts,
            [Request::Toast {
                title: "Text Grab".into(),
                text: "Tesseract is not installed.".into(),
                ms: 6000
            }]
        );
    }

    fn reviewed(light: bool) -> (tempfile::TempDir, Harness<'static, State<TextGrabModule>>) {
        let dir = tempfile::tempdir().unwrap();
        let set = [("textgrab.review", serde_json::json!(true))];
        let (mut h, _) = opened(dir.path(), light, true, &set, Ok("Invoice 2041\nTotal: £18.40"));
        press_and_move(&mut h, pos2(10.0, 10.0), pos2(200.0, 100.0));
        release(&mut h, pos2(200.0, 100.0));
        assert_eq!(h.state().module.review_text(), Some("Invoice 2041\nTotal: £18.40"));
        (dir, h)
    }

    #[test]
    fn the_review_box_dark() {
        let (_dir, mut h) = reviewed(false);
        photograph(&mut h, "text_grab_review_dark");
    }

    #[test]
    fn the_review_box_light_copies_with_ctrl_enter() {
        let (_dir, mut h) = reviewed(true);
        photograph(&mut h, "text_grab_review_light");
        assert!(notes(h.state()).is_empty(), "nothing is copied before Copy");
        h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::Enter);
        h.run();
        assert_eq!(notes(h.state()), ["Copied."]);
        for _ in 0..30 {
            h.step();
        }
        assert_eq!(h.state().module.review_text(), None);
    }
}
