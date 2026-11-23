//! The app with a display: eframe owns the main thread and the core [`App`] lives inside it.
//!
//! eframe needs a main window, but Luminescent has none: every window it shows is a viewport of its own
//! (overlays, panels, toasts). So the main window is a 1×1 transparent one that is never seen. Nothing paints
//! while nothing happens: services post a message and call `request_repaint`, which runs [`eframe::App::logic`]
//! once, and the usage flush asks for a single wake-up when it is due.

use std::cell::RefCell;
use std::panic::AssertUnwindSafe;
use std::rc::Rc;
use std::str::FromStr;
use std::sync::atomic::Ordering;
use std::time::Instant;

use egui::{ViewportBuilder, ViewportCommand, ViewportId};
use lum_core::APP_NAME;
use lum_platform::desktop;

use crate::launcher::App;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Renderer {
    #[default]
    Glow,
    Wgpu,
}

impl FromStr for Renderer {
    type Err = String;

    fn from_str(s: &str) -> Result<Renderer, String> {
        match s.to_ascii_lowercase().as_str() {
            "glow" | "gl" | "opengl" => Ok(Renderer::Glow),
            "wgpu" => Ok(Renderer::Wgpu),
            other => Err(format!("unknown renderer '{other}' (glow or wgpu)")),
        }
    }
}

impl Renderer {
    pub fn name(self) -> &'static str {
        match self {
            Renderer::Glow => "glow",
            Renderer::Wgpu => "wgpu",
        }
    }

    /// eframe's renderer, or why this build can not use it.
    fn eframe(self) -> Result<eframe::Renderer, String> {
        match self {
            Renderer::Glow => Ok(eframe::Renderer::Glow),
            #[cfg(feature = "wgpu")]
            Renderer::Wgpu => Ok(eframe::Renderer::Wgpu),
            #[cfg(not(feature = "wgpu"))]
            Renderer::Wgpu => Err("this build has no wgpu: build it with --features wgpu".to_owned()),
        }
    }
}

/// eframe's options for a program whose windows are all viewports of their own.
pub fn native_options(renderer: Renderer, root: ViewportBuilder) -> Result<eframe::NativeOptions, String> {
    Ok(eframe::NativeOptions {
        viewport: root,
        renderer: renderer.eframe()?,
        // remembering the main window's place makes no sense for a window nobody sees
        persist_window: false,
        centered: false,
        ..Default::default()
    })
}

pub fn hidden_root() -> ViewportBuilder {
    ViewportBuilder::default()
        .with_title(desktop::ROOT_TITLE)
        .with_app_id(desktop::APP_ID)
        .with_inner_size([1.0, 1.0])
        .with_decorations(false)
        .with_transparent(true)
        .with_taskbar(false)
        .with_active(false)
        .with_visible(false)
}

/// Runs the app until something asks it to quit, and hands it back. Without a working renderer it falls back
/// to running without windows, so the ctl socket keeps working.
pub fn run(app: App, renderer: Renderer) -> (App, u8) {
    let shared = Rc::new(RefCell::new(app));
    let started = Rc::new(RefCell::new(false));
    let result = native_options(renderer, hidden_root()).and_then(|options| {
        if let Err(e) = desktop::install_rules(desktop::APP_ID, &[]) {
            tracing::warn!("could not set the window rules: {e}");
        }
        shared.borrow_mut().mode = format!("hidden main window, {}", renderer.name());
        let (app, flag) = (shared.clone(), started.clone());
        // a missing system library makes winit panic rather than fail, and that must not take the app down
        let ran = std::panic::catch_unwind(AssertUnwindSafe(|| {
            eframe::run_native(
                APP_NAME,
                options,
                Box::new(move |cc| {
                    *flag.borrow_mut() = true;
                    lum_ui::setup(&cc.egui_ctx, lum_ui::Tokens::dark());
                    app.borrow().waker.set(&cc.egui_ctx);
                    Ok(Box::new(Shell { app, frames: 0 }))
                }),
            )
        }));
        desktop::remove_rules(desktop::APP_ID);
        match ran {
            Ok(result) => result.map_err(|e| e.to_string()),
            Err(_) => Err("the window system panicked (see crash.log)".to_owned()),
        }
    });
    let mut app =
        Rc::into_inner(shared).expect("eframe drops the shell when run_native returns").into_inner();
    match result {
        Ok(()) => {}
        Err(e) if !*started.borrow() => {
            tracing::error!("could not open a window ({e}): running without windows");
            app.mode = "no windows (the renderer failed)".to_owned();
            app.run_headless();
        }
        Err(e) => {
            tracing::error!("the window system failed: {e}");
            app.exit_code = app.exit_code.max(1);
        }
    }
    let code = app.exit_code;
    (app, code)
}

struct Shell {
    app: Rc<RefCell<App>>,
    frames: u64,
}

impl eframe::App for Shell {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let mut app = self.app.borrow_mut();
        app.passes.logic.fetch_add(1, Ordering::Relaxed);
        let next = app.poll();
        if app.quitting {
            ctx.send_viewport_cmd_to(ViewportId::ROOT, ViewportCommand::Close);
        } else if let Some(at) = next {
            ctx.request_repaint_after(at.saturating_duration_since(Instant::now()));
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.app.borrow().passes.ui.fetch_add(1, Ordering::Relaxed);
        self.frames += 1;
        if self.frames == 1 {
            // eframe shows the main window once its first frame is painted; this hides it again right after
            ui.ctx().send_viewport_cmd(ViewportCommand::Visible(false));
        }
    }

    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        [0.0; 4]
    }
}
