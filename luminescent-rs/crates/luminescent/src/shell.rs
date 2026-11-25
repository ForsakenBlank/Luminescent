//! The app with a display: eframe owns the main thread and the core [`App`] lives inside it.
//!
//! eframe needs a main window, but Luminescent has none: every window it shows is a viewport of its own
//! (overlays, panels, toasts). So the main window is a 1x1 transparent one that is never seen. Nothing paints
//! while nothing happens: services post a message and call `request_repaint`, which runs [`eframe::App::logic`]
//! once, and the usage flush asks for a single wake-up when it is due.
//!
//! The main window stays "visible" as far as winit knows. eframe paints a hidden main window from outside the
//! event loop, where a window opened from it (every overlay and panel is drawn from the main window's pass)
//! can not be created, and a repaint of any of those windows is a repaint of the main window. So it is a
//! transparent, click-through pixel, off screen where the system lets us put it.

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

/// What draws the pixels, from the renderer itself.
#[derive(Debug, Clone, Default)]
pub struct Gpu {
    pub name: String,
    /// Drawn by the CPU (llvmpipe, lavapipe, WARP). Its memory then counts as ours, so `--idle-check` does not
    /// hold the memory budget against it.
    pub software: bool,
}

impl Gpu {
    pub fn of(cc: &eframe::CreationContext<'_>) -> Gpu {
        if let Some(gl) = &cc.gl {
            use eframe::glow::HasContext as _;
            // SAFETY: a plain query on the context eframe just made current on this thread.
            let name = unsafe { gl.get_parameter_string(eframe::glow::RENDERER) };
            let lower = name.to_ascii_lowercase();
            let software = ["llvmpipe", "softpipe", "swrast", "software", "gdi generic"]
                .iter()
                .any(|s| lower.contains(s));
            return Gpu { name, software };
        }
        #[cfg(feature = "wgpu")]
        if let Some(state) = &cc.wgpu_render_state {
            let info = state.adapter.get_info();
            let software = info.device_type == eframe::wgpu::DeviceType::Cpu;
            return Gpu { name: format!("{} ({:?})", info.name, info.backend), software };
        }
        Gpu { name: "unknown".to_owned(), software: false }
    }

    pub fn describe(&self) -> String {
        if self.software { format!("{} (software)", self.name) } else { self.name.clone() }
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
        // far outside every screen on X11 and Windows; on Hyprland the rule from install_rules places it
        .with_position([-30_000.0, -30_000.0])
        .with_decorations(false)
        .with_transparent(true)
        .with_mouse_passthrough(true)
        .with_resizable(false)
        .with_taskbar(false)
        .with_active(false)
}

/// Runs the app until something asks it to quit, and hands it back. Without a working renderer it falls back
/// to running without windows, so the ctl socket keeps working.
pub fn run(app: App, renderer: Renderer) -> (App, u8) {
    let shared = Rc::new(RefCell::new(app));
    let started = Rc::new(RefCell::new(false));
    let result = native_options(renderer, hidden_root()).and_then(|options| {
        let (app, flag) = (shared.clone(), started.clone());
        // a missing system library makes winit panic rather than fail, and that must not take the app down
        let ran = std::panic::catch_unwind(AssertUnwindSafe(|| {
            eframe::run_native(
                APP_NAME,
                options,
                Box::new(move |cc| {
                    *flag.borrow_mut() = true;
                    {
                        let core = app.borrow();
                        lum_ui::setup(&cc.egui_ctx, core.tokens());
                        lum_ui::Motion::from_settings(&core.settings).install(&cc.egui_ctx);
                        lum_ui::window::install_placer(&cc.egui_ctx, core.desktop.placer());
                    }
                    let gpu = Gpu::of(cc);
                    tracing::info!("drawing with {} on {}", renderer.name(), gpu.describe());
                    let mut core = app.borrow_mut();
                    core.mode = format!("hidden main window, {} on {}", renderer.name(), gpu.describe());
                    core.software_renderer = gpu.software;
                    core.waker.set(&cc.egui_ctx);
                    drop(core);
                    Ok(Box::new(Shell { app }))
                }),
            )
        }));
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
}

impl eframe::App for Shell {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let mut app = self.app.borrow_mut();
        app.passes.logic.fetch_add(1, Ordering::Relaxed);
        let next = app.poll();
        // a theme switch: every window draws with the new tokens from this pass on
        if let Some(tokens) = app.take_restyle() {
            tokens.install(ctx);
            ctx.request_repaint();
        }
        if app.quitting {
            ctx.send_viewport_cmd_to(ViewportId::ROOT, ViewportCommand::Close);
        } else if let Some(at) = next {
            ctx.request_repaint_after(at.saturating_duration_since(Instant::now()));
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let mut app = self.app.borrow_mut();
        app.passes.ui.fetch_add(1, Ordering::Relaxed);
        // every window of ours is drawn from here, so a repaint of any of them runs this
        app.show_modules(ui.ctx());
    }

    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        [0.0; 4]
    }
}
