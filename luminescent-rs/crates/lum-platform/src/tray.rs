//! The tray icon: the sparkle in the theme's accent, a tooltip and a menu built by the app. A
//! StatusNotifierItem on Linux (it needs a tray host such as Caelestia's bar or waybar; GNOME needs the
//! AppIndicator extension), a notification-area icon on Windows.
//!
//! The icon lives on its own thread on both systems (Windows wants a message loop where it was made), so
//! changing the menu or the tooltip never waits on D-Bus or the shell.

use std::sync::Arc;
use tiny_skia::{FillRule, Paint, PathBuilder, Pixmap, Transform};

/// One menu entry. Ids come back in [`TrayEvent::Menu`].
#[derive(Debug, Clone, PartialEq)]
pub enum MenuItem {
    /// Greyed-out text (the heading with the version).
    Label(String),
    Action { id: String, text: String, enabled: bool },
    Check { id: String, text: String, checked: bool },
    Sub { text: String, items: Vec<MenuItem> },
    Separator,
}

impl MenuItem {
    pub fn action(id: &str, text: impl Into<String>) -> MenuItem {
        MenuItem::Action { id: id.to_owned(), text: text.into(), enabled: true }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrayEvent {
    /// The icon was clicked (the overlay opens or closes).
    Activated,
    Menu(String),
}

/// A square picture, straight (not premultiplied) RGBA.
#[derive(Debug, Clone, PartialEq)]
pub struct Icon {
    pub size: u32,
    pub rgba: Vec<u8>,
}

/// The Luminescent mark (a four-pointed sparkle with curved sides) in `rgb` on a transparent background, the
/// same path as `lum_ui::paint::sparkle`, with the Python icon's 4 % margin.
pub fn sparkle_icon(size: u32, rgb: [u8; 3]) -> Icon {
    let size = size.max(1);
    let mut pixmap = Pixmap::new(size, size).expect("a non-empty square");
    let pad = size as f32 * 0.04;
    let s = (size as f32 - 2.0 * pad) / 100.0;
    let pt = |a: f32, b: f32| (pad + a * s, pad + b * s);
    let mut path = PathBuilder::new();
    let start = pt(50.0, 0.0);
    path.move_to(start.0, start.1);
    let curves = [
        [(54.0, 34.0), (66.0, 46.0), (100.0, 50.0)],
        [(66.0, 54.0), (54.0, 66.0), (50.0, 100.0)],
        [(46.0, 66.0), (34.0, 54.0), (0.0, 50.0)],
        [(34.0, 46.0), (46.0, 34.0), (50.0, 0.0)],
    ];
    for [c1, c2, end] in curves {
        let (c1, c2, end) = (pt(c1.0, c1.1), pt(c2.0, c2.1), pt(end.0, end.1));
        path.cubic_to(c1.0, c1.1, c2.0, c2.1, end.0, end.1);
    }
    path.close();
    if let Some(path) = path.finish() {
        let mut paint = Paint::default();
        paint.set_color_rgba8(rgb[0], rgb[1], rgb[2], 255);
        paint.anti_alias = true;
        pixmap.fill_path(&path, &paint, FillRule::Winding, Transform::identity(), None);
    }
    let rgba = pixmap.pixels().iter().flat_map(|p| {
        let c = p.demultiply();
        [c.red(), c.green(), c.blue(), c.alpha()]
    });
    Icon { size, rgba: rgba.collect() }
}

/// The sizes a tray host may ask for.
pub fn sparkle_icons(rgb: [u8; 3]) -> Vec<Icon> {
    [16, 20, 24, 32, 48, 64, 128, 256].into_iter().map(|s| sparkle_icon(s, rgb)).collect()
}

/// The tooltip's lines joined, kept short enough for every tray host.
pub fn tooltip_text(lines: &[String]) -> String {
    let text = lines.join("\n");
    if text.chars().count() > 127 {
        let mut short: String = text.chars().take(126).collect();
        short.push('…');
        return short;
    }
    text
}

enum Cmd {
    Menu(Vec<MenuItem>),
    Tooltip(String),
    Icons(Vec<Icon>),
    Stop,
}

pub type Sink = Arc<dyn Fn(TrayEvent) + Send + Sync>;

/// The running tray icon. Dropping it does not take the icon away; call [`Tray::stop`].
pub struct Tray {
    tx: crossbeam_channel::Sender<Cmd>,
    #[cfg(windows)]
    thread: u32,
}

impl Tray {
    pub fn start(tooltip: String, icons: Vec<Icon>, menu: Vec<MenuItem>, sink: Sink) -> Result<Tray, String> {
        imp::start(tooltip, icons, menu, sink)
    }

    fn send(&self, cmd: Cmd) {
        let _ = self.tx.send(cmd);
        #[cfg(windows)]
        imp::wake(self.thread);
    }

    pub fn set_menu(&self, menu: Vec<MenuItem>) {
        self.send(Cmd::Menu(menu));
    }

    pub fn set_tooltip(&self, text: String) {
        self.send(Cmd::Tooltip(text));
    }

    pub fn set_icons(&self, icons: Vec<Icon>) {
        self.send(Cmd::Icons(icons));
    }

    pub fn stop(&self) {
        self.send(Cmd::Stop);
    }
}

#[cfg(target_os = "linux")]
mod imp {
    use super::{Cmd, Icon, MenuItem, Sink, Tray, TrayEvent};
    use ksni::blocking::TrayMethods;

    struct Sni {
        tooltip: String,
        icons: Vec<Icon>,
        menu: Vec<MenuItem>,
        sink: Sink,
    }

    fn argb(icon: &Icon) -> ksni::Icon {
        let data = icon.rgba.chunks_exact(4).flat_map(|p| [p[3], p[0], p[1], p[2]]).collect();
        ksni::Icon { width: icon.size as i32, height: icon.size as i32, data }
    }

    fn items(list: &[MenuItem]) -> Vec<ksni::MenuItem<Sni>> {
        list.iter()
            .map(|item| match item {
                MenuItem::Label(text) => {
                    ksni::menu::StandardItem { label: text.clone(), enabled: false, ..Default::default() }.into()
                }
                MenuItem::Action { id, text, enabled } => {
                    let id = id.clone();
                    ksni::menu::StandardItem {
                        label: text.clone(),
                        enabled: *enabled,
                        activate: Box::new(move |t: &mut Sni| (t.sink)(TrayEvent::Menu(id.clone()))),
                        ..Default::default()
                    }
                    .into()
                }
                MenuItem::Check { id, text, checked } => {
                    let id = id.clone();
                    ksni::menu::CheckmarkItem {
                        label: text.clone(),
                        checked: *checked,
                        activate: Box::new(move |t: &mut Sni| (t.sink)(TrayEvent::Menu(id.clone()))),
                        ..Default::default()
                    }
                    .into()
                }
                MenuItem::Sub { text, items: sub } => {
                    ksni::menu::SubMenu { label: text.clone(), submenu: items(sub), ..Default::default() }.into()
                }
                MenuItem::Separator => ksni::MenuItem::Separator,
            })
            .collect()
    }

    impl ksni::Tray for Sni {
        fn id(&self) -> String {
            "luminescent".to_owned()
        }

        fn title(&self) -> String {
            lum_core::APP_NAME.to_owned()
        }

        fn icon_pixmap(&self) -> Vec<ksni::Icon> {
            self.icons.iter().map(argb).collect()
        }

        fn tool_tip(&self) -> ksni::ToolTip {
            ksni::ToolTip { title: self.tooltip.clone(), ..Default::default() }
        }

        fn activate(&mut self, _x: i32, _y: i32) {
            (self.sink)(TrayEvent::Activated);
        }

        fn menu(&self) -> Vec<ksni::MenuItem<Self>> {
            items(&self.menu)
        }
    }

    pub fn start(tooltip: String, icons: Vec<Icon>, menu: Vec<MenuItem>, sink: Sink) -> Result<Tray, String> {
        let (tx, rx) = crossbeam_channel::unbounded();
        let (ready_tx, ready_rx) = crossbeam_channel::bounded(1);
        std::thread::Builder::new()
            .name("tray".into())
            .spawn(move || {
                let handle = match (Sni { tooltip, icons, menu, sink }).spawn() {
                    Ok(handle) => {
                        let _ = ready_tx.send(Ok(()));
                        handle
                    }
                    Err(e) => {
                        let _ = ready_tx.send(Err(e.to_string()));
                        return;
                    }
                };
                for cmd in rx {
                    match cmd {
                        Cmd::Menu(menu) => drop(handle.update(|t| t.menu = menu)),
                        Cmd::Tooltip(text) => drop(handle.update(|t| t.tooltip = text)),
                        Cmd::Icons(icons) => drop(handle.update(|t| t.icons = icons)),
                        Cmd::Stop => break,
                    }
                }
                drop(handle.shutdown());
            })
            .map_err(|e| e.to_string())?;
        ready_rx.recv().map_err(|e| e.to_string())??;
        Ok(Tray { tx })
    }
}

#[cfg(windows)]
mod imp {
    use super::{Cmd, Icon, MenuItem, Sink, Tray, TrayEvent};
    use tray_icon::menu::{CheckMenuItem, IsMenuItem, Menu, MenuEvent, MenuId, PredefinedMenuItem, Submenu};
    use tray_icon::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};
    use windows_sys::Win32::System::Threading::GetCurrentThreadId;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        DispatchMessageW, GetMessageW, MSG, PostThreadMessageW, TranslateMessage, WM_APP,
    };

    const WAKE: u32 = WM_APP + 0x4C;

    pub fn wake(thread: u32) {
        // SAFETY: posting a plain message to a thread id we got from that thread; a gone thread just fails.
        unsafe { PostThreadMessageW(thread, WAKE, 0, 0) };
    }

    fn icon(icons: &[Icon]) -> Option<tray_icon::Icon> {
        // Windows shows the small icon at 16 to 32 pixels; it scales the 32 one down nicely
        let best = icons.iter().find(|i| i.size == 32).or_else(|| icons.first())?;
        tray_icon::Icon::from_rgba(best.rgba.clone(), best.size, best.size).ok()
    }

    fn add(parent: &dyn Fn(&dyn IsMenuItem), list: &[MenuItem]) {
        for item in list {
            match item {
                MenuItem::Label(text) => parent(&tray_icon::menu::MenuItem::new(text, false, None)),
                MenuItem::Action { id, text, enabled } => {
                    parent(&tray_icon::menu::MenuItem::with_id(MenuId::new(id), text, *enabled, None))
                }
                MenuItem::Check { id, text, checked } => {
                    parent(&CheckMenuItem::with_id(MenuId::new(id), text, true, *checked, None))
                }
                MenuItem::Sub { text, items } => {
                    let sub = Submenu::new(text, true);
                    add(&|i: &dyn IsMenuItem| drop(sub.append(i)), items);
                    parent(&sub);
                }
                MenuItem::Separator => parent(&PredefinedMenuItem::separator()),
            }
        }
    }

    fn menu(list: &[MenuItem]) -> Menu {
        let menu = Menu::new();
        add(&|i: &dyn IsMenuItem| drop(menu.append(i)), list);
        menu
    }

    pub fn start(tooltip: String, icons: Vec<Icon>, items: Vec<MenuItem>, sink: Sink) -> Result<Tray, String> {
        let (tx, rx) = crossbeam_channel::unbounded::<Cmd>();
        let (ready_tx, ready_rx) = crossbeam_channel::bounded(1);
        let clicks = sink.clone();
        TrayIconEvent::set_event_handler(Some(move |e: TrayIconEvent| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = e {
                clicks(TrayEvent::Activated);
            }
        }));
        MenuEvent::set_event_handler(Some(move |e: MenuEvent| sink(TrayEvent::Menu(e.id.0))));
        std::thread::Builder::new()
            .name("tray".into())
            .spawn(move || {
                let built = icon(&icons).ok_or_else(|| "no icon".to_owned()).and_then(|icon| {
                    TrayIconBuilder::new()
                        .with_tooltip(&tooltip)
                        .with_menu(Box::new(menu(&items)))
                        .with_icon(icon)
                        .build()
                        .map_err(|e| e.to_string())
                });
                let tray: TrayIcon = match built {
                    Ok(tray) => tray,
                    Err(e) => {
                        let _ = ready_tx.send(Err(e));
                        return;
                    }
                };
                // SAFETY: no preconditions.
                let _ = ready_tx.send(Ok(unsafe { GetCurrentThreadId() }));
                // SAFETY: MSG is plain data that GetMessageW fills; the loop ends on WM_QUIT (0) or an error.
                let mut msg = unsafe { std::mem::zeroed::<MSG>() };
                // SAFETY: as above.
                while unsafe { GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) } > 0 {
                    if msg.message == WAKE {
                        for cmd in rx.try_iter() {
                            match cmd {
                                Cmd::Menu(items) => tray.set_menu(Some(Box::new(menu(&items)))),
                                Cmd::Tooltip(text) => drop(tray.set_tooltip(Some(text))),
                                Cmd::Icons(icons) => drop(tray.set_icon(icon(&icons))),
                                Cmd::Stop => return,
                            }
                        }
                        continue;
                    }
                    // SAFETY: a message GetMessageW just filled in, handed back the usual way.
                    unsafe {
                        TranslateMessage(&msg);
                        DispatchMessageW(&msg);
                    }
                }
            })
            .map_err(|e| e.to_string())?;
        let thread = ready_rx.recv().map_err(|e| e.to_string())??;
        Ok(Tray { tx, thread })
    }
}

#[cfg(not(any(target_os = "linux", windows)))]
mod imp {
    use super::{Icon, MenuItem, Sink, Tray};

    pub fn start(_tooltip: String, _icons: Vec<Icon>, _menu: Vec<MenuItem>, _sink: Sink) -> Result<Tray, String> {
        Err("no tray on this system".to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_sparkle_is_drawn_in_the_accent() {
        let icon = sparkle_icon(64, [0x7B, 0x6C, 0xF6]);
        assert_eq!(icon.rgba.len(), 64 * 64 * 4);
        let at = |x: usize, y: usize| &icon.rgba[(y * 64 + x) * 4..(y * 64 + x) * 4 + 4];
        assert_eq!(at(32, 32), [0x7B, 0x6C, 0xF6, 255], "solid in the middle");
        assert_eq!(at(2, 2)[3], 0, "clear in the corners");
        assert_eq!(at(10, 10)[3], 0, "the sides curve in");
        assert_eq!(sparkle_icons([1, 2, 3]).len(), 8);
    }

    #[test]
    fn long_tooltips_are_cut() {
        assert_eq!(tooltip_text(&["Luminescent".into(), "Keep awake is on".into()]), "Luminescent\nKeep awake is on");
        let long = tooltip_text(&["x".repeat(200)]);
        assert_eq!(long.chars().count(), 127);
        assert!(long.ends_with('…'));
    }
}
