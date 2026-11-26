//! The command palette: one search box for everything (`ui/palette.py`).
//!
//! The app hands it the index (pages, module actions, dashboard items, themes, settings sections, ...) each time
//! it opens, so it is never stale and costs nothing while closed. Matching is fuzzy, with nucleo-matcher; the
//! haystacks are made once per open, so a keystroke over a couple of thousand entries takes well under a
//! millisecond. What you ran last comes first. The app can add rows for what is typed (a sum, "remind me in
//! 20m to ...") with the `extra` function it passes to [`Palette::show`].

use std::path::PathBuf;

use egui::text::{CCursor, CCursorRange};
use egui::{
    Align, Area, Context, Id, Key, Layout, Modifiers, Order, Rect, Sense, Stroke, StrokeKind, TextEdit, Ui,
    UiBuilder, ViewportBuilder, ViewportId, pos2, vec2,
};
use lum_core::pyjson::{self, Style};
use lum_core::store::{self, Writer};
use lum_platform::desktop::APP_ID;
use nucleo_matcher::pattern::{CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Matcher, Utf32String};
use serde_json::{Value, json};

use crate::anim::{self, Ease, Kind};
use crate::fonts::Weight;
use crate::paint;
use crate::text::Text;
use crate::theme::{Role, Tokens, Tone, alpha};

pub const MAX_SHOWN: usize = 60;
const ROW_H: f32 = 40.0;
const GROUP_W: f32 = 76.0;
/// The see-through margin around the box for the painted glow.
const GLOW: f32 = 36.0;
const WIDTH: f32 = 600.0;
const HEAD_H: f32 = 56.0;
const FOOT_H: f32 = 34.0;
const LIST_H: f32 = ROW_H * 9.0 + 12.0;
const BOX_H: f32 = 1.0 + HEAD_H + LIST_H + 12.0 + FOOT_H + 1.0;
const RECENT_KEPT: usize = 20;
const TITLE: &str = "Luminescent palette";

/// One row of the palette. The app knows what to do with its `id` when it comes back from [`Palette::show`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub id: String,
    pub label: String,
    pub hint: String,
    pub kind: String,
}

impl Entry {
    pub fn new(
        id: impl Into<String>,
        label: impl Into<String>,
        hint: impl Into<String>,
        kind: &str,
    ) -> Entry {
        Entry { id: id.into(), label: label.into(), hint: hint.into(), kind: kind.to_owned() }
    }

    /// The group shown on the left in mono caps, and the keys shown on the right.
    pub fn columns(&self) -> (String, String) {
        let parts: Vec<&str> = self.hint.split(" · ").map(str::trim).collect();
        let second = || parts.get(1).map_or(String::new(), |s| (*s).to_owned());
        match self.kind.as_str() {
            "action" | "item" => (parts[0].to_uppercase(), second()),
            "theme" => ("THEME".into(), if self.hint == "active" { "ACTIVE".into() } else { String::new() }),
            "calc" => ("CALC".into(), "ENTER COPIES".into()),
            "reminder" => ("REMIND".into(), second()),
            kind => {
                let group = match kind {
                    "page" => "OVERLAY",
                    "settings" => "SETTINGS",
                    "luminescent" => "LUMINESCENT",
                    "note" => "NOTES",
                    "colour" => "COLOUR",
                    "pin" => "CALCULATOR",
                    _ => "",
                };
                let group = if group.is_empty() {
                    if parts[0].is_empty() { kind.to_uppercase() } else { parts[0].to_uppercase() }
                } else {
                    group.to_owned()
                };
                (group, second())
            }
        }
    }

    /// Runs leave a trace in the recent list (a sum or a quick reminder does not).
    fn remembered(&self) -> bool {
        !(self.id.starts_with("calc:") || self.id.starts_with("remind:"))
    }
}

struct Indexed {
    entry: Entry,
    haystack: Utf32String,
}

/// The fuzzy matching on its own, so it can be tested and timed without a window.
pub struct Search {
    entries: Vec<Indexed>,
    matcher: Matcher,
}

impl Default for Search {
    fn default() -> Search {
        Search { entries: Vec::new(), matcher: Matcher::new(nucleo_matcher::Config::DEFAULT) }
    }
}

impl Search {
    pub fn new(entries: Vec<Entry>) -> Search {
        let entries = entries
            .into_iter()
            .map(|entry| {
                let haystack = Utf32String::from(format!("{} {}", entry.label, entry.hint).as_str());
                Indexed { entry, haystack }
            })
            .collect();
        Search { entries, ..Search::default() }
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The entries for `query`, best first, and how many matched in all. An empty query lists the recent ones
    /// first, then the rest in the order given. Recently run entries also get a lift when they match.
    pub fn results(&mut self, query: &str, recent: &[String]) -> (Vec<Entry>, usize) {
        if query.trim().is_empty() {
            let mut out: Vec<Entry> = recent
                .iter()
                .filter_map(|id| self.entries.iter().find(|e| e.entry.id == *id))
                .map(|e| e.entry.clone())
                .collect();
            let total = self.entries.len();
            for e in &self.entries {
                if out.len() >= MAX_SHOWN {
                    break;
                }
                if !recent.contains(&e.entry.id) {
                    out.push(e.entry.clone());
                }
            }
            return (out, total);
        }
        let pattern = Pattern::parse(query, CaseMatching::Ignore, Normalization::Smart);
        let mut scored: Vec<(u32, usize)> = Vec::new();
        for (i, e) in self.entries.iter().enumerate() {
            if let Some(score) = pattern.score(e.haystack.slice(..), &mut self.matcher) {
                // newer runs weigh more, like Python's `rank * 0.8`; nucleo's points are about twice as big
                let rank = recent.iter().position(|id| *id == e.entry.id).map_or(0, |at| recent.len() - at);
                scored.push((score + rank as u32 * 2, i));
            }
        }
        // the sort is stable, so equal scores keep the index's order
        scored.sort_by_key(|(score, _)| std::cmp::Reverse(*score));
        let total = scored.len();
        let out = scored.into_iter().take(MAX_SHOWN).map(|(_, i)| self.entries[i].entry.clone()).collect();
        (out, total)
    }
}

pub struct Palette {
    open: bool,
    text: String,
    search: Search,
    recent: Vec<String>,
    results: Vec<Entry>,
    total: usize,
    selected: usize,
    /// The query the results were made for.
    shown_for: Option<String>,
    /// The screen it opens on, in desktop coordinates (`None`: the one the system reports for the main window).
    screen: Option<Rect>,
    serial: u64,
    fresh: bool,
    had_focus: bool,
    scroll_to_selected: bool,
    recent_path: PathBuf,
    writer: Writer,
}

impl std::fmt::Debug for Palette {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Palette").field("open", &self.open).field("text", &self.text).finish()
    }
}

fn base_id() -> Id {
    Id::unique("lum-palette")
}

impl Palette {
    /// `recent_path` is `palette.json`, which holds `{"recent": [ids]}`.
    pub fn new(recent_path: PathBuf) -> Palette {
        let recent = store::load_json(&recent_path)
            .and_then(|v| v.get("recent").and_then(Value::as_array).cloned())
            .map(|list| list.iter().filter_map(Value::as_str).map(str::to_owned).collect())
            .unwrap_or_default();
        Palette {
            open: false,
            text: String::new(),
            search: Search::default(),
            recent,
            results: Vec::new(),
            total: 0,
            selected: 0,
            shown_for: None,
            screen: None,
            serial: 0,
            fresh: false,
            had_focus: false,
            scroll_to_selected: false,
            recent_path,
            writer: Writer::new("palette"),
        }
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn results(&self) -> &[Entry] {
        &self.results
    }

    /// Opens it over `screen` with a fresh index; `text` carries on a search typed on the overlay.
    pub fn open(&mut self, entries: Vec<Entry>, text: &str, screen: Option<Rect>) {
        self.search = Search::new(entries);
        self.text = text.to_owned();
        self.shown_for = None;
        self.selected = 0;
        self.screen = screen;
        self.open = true;
        self.fresh = true;
        self.had_focus = false;
        self.serial += 1;
    }

    pub fn close(&mut self) {
        self.open = false;
        self.search = Search::default();
        self.results.clear();
    }

    /// Writes the recent list (at quit; runs save it as they happen).
    pub fn flush(&self) {
        self.writer.flush();
    }

    fn refresh(&mut self, extra: &dyn Fn(&str) -> Vec<Entry>) {
        if self.shown_for.as_deref() == Some(self.text.as_str()) {
            return;
        }
        let (found, total) = self.search.results(&self.text, &self.recent);
        let mut results = extra(&self.text);
        let extra_count = results.len();
        results.extend(found);
        results.truncate(MAX_SHOWN);
        self.results = results;
        self.total = total + extra_count;
        self.selected = 0;
        self.scroll_to_selected = true;
        self.shown_for = Some(self.text.clone());
    }

    /// Draws it for this frame (nothing while it is closed). Gives back the entry to run, once it is chosen; the
    /// palette has closed by then.
    pub fn show(&mut self, ctx: &Context, extra: &dyn Fn(&str) -> Vec<Entry>) -> Option<Entry> {
        if !self.open {
            return None;
        }
        self.refresh(extra);
        let size = vec2(WIDTH + 2.0 * GLOW, BOX_H + 2.0 * GLOW);
        let screen = self.screen.unwrap_or_else(|| {
            if ctx.embed_viewports() {
                ctx.content_rect()
            } else {
                let size = ctx.input(|i| i.viewport().monitor_size).unwrap_or(vec2(1280.0, 800.0));
                Rect::from_min_size(pos2(0.0, 0.0), size)
            }
        });
        let at = pos2(
            (screen.center().x - size.x / 2.0).round(),
            (screen.min.y + screen.height() * 0.18 - GLOW).round(),
        );
        let mut chosen = None;
        let mut close = false;
        if ctx.embed_viewports() {
            let origin = ctx.content_rect().min + (at - screen.min);
            Area::new(base_id().with("embedded")).order(Order::Foreground).fixed_pos(origin).show(
                ctx,
                |ui| {
                    let rect = Rect::from_min_size(ui.cursor().min, size);
                    ui.allocate_rect(rect, Sense::hover());
                    let mut child = ui.new_child(UiBuilder::new().max_rect(rect));
                    (chosen, close) = self.draw(&mut child, extra);
                },
            );
        } else {
            let builder = ViewportBuilder::default()
                .with_title(TITLE)
                .with_app_id(APP_ID)
                .with_decorations(false)
                .with_transparent(true)
                .with_always_on_top()
                .with_taskbar(false)
                .with_resizable(false)
                .with_active(true)
                .with_position(at)
                .with_inner_size(size);
            ctx.show_viewport_immediate(ViewportId::from_hash_of(base_id()), builder, |ui, _| {
                // it closes when it loses the keyboard to another window, like a menu
                let focused = ui.input(|i| i.viewport().focused);
                if focused == Some(true) {
                    self.had_focus = true;
                } else if focused == Some(false) && self.had_focus {
                    close = true;
                }
                let rect = ui.max_rect();
                let mut child = ui.new_child(UiBuilder::new().max_rect(rect));
                let (c, x) = self.draw(&mut child, extra);
                if c.is_some() {
                    chosen = c;
                }
                close |= x;
            });
            crate::window::shown(ctx, TITLE, Some(at), true);
            if std::mem::take(&mut self.fresh) {
                crate::window::placer(ctx).focus(TITLE);
            }
        }
        if let Some(entry) = &chosen {
            self.ran(entry);
        }
        if close || chosen.is_some() {
            self.close();
            ctx.request_repaint();
        }
        chosen
    }

    fn ran(&mut self, entry: &Entry) {
        if !entry.remembered() {
            return;
        }
        self.recent.retain(|id| *id != entry.id);
        self.recent.insert(0, entry.id.clone());
        self.recent.truncate(RECENT_KEPT);
        let text = pyjson::to_string(&json!({"recent": self.recent}), Style::COMPACT);
        self.writer.save(&self.recent_path, text);
    }

    fn draw(&mut self, ui: &mut Ui, extra: &dyn Fn(&str) -> Vec<Entry>) -> (Option<Entry>, bool) {
        let tokens = Tokens::of(ui.ctx());
        let p = &tokens.p;
        let outer = ui.max_rect();
        let bx = outer.shrink(GLOW);
        let fade = anim::value_from(
            ui.ctx(),
            base_id().with(("in", self.serial)),
            0.0,
            1.0,
            Kind::Panels,
            140.0,
            Ease::Out,
        );
        ui.multiply_opacity(fade);
        if tokens.glow() {
            let r = tokens.r();
            for i in 1..12 {
                let a = tokens.glow_alpha() * 0.22 * (1.0 - i as f32 / 12.0).powi(2);
                let d = i as f32 * 3.0;
                ui.painter().rect_stroke(
                    bx.expand(d),
                    r + d,
                    Stroke::new(3.0, alpha(p.primary, a)),
                    StrokeKind::Middle,
                );
            }
            ui.painter().rect_stroke(
                bx.expand(1.5),
                r + 1.5,
                Stroke::new(1.0, alpha(p.primary, 0.16)),
                StrokeKind::Middle,
            );
        }
        paint::fill(ui.painter(), bx, tokens.r(), &tokens, Role::BgBase, 1.0);
        ui.painter().rect_stroke(bx, tokens.r(), Stroke::new(1.0, p.border), StrokeKind::Inside);

        let (escape, step, tab, enter) = ui.input_mut(|i| {
            let mut step = 0i64;
            for (key, by) in [(Key::ArrowDown, 1), (Key::ArrowUp, -1), (Key::PageDown, 8), (Key::PageUp, -8)]
            {
                while i.consume_key(Modifiers::NONE, key) {
                    step += by;
                }
            }
            (
                i.consume_key(Modifiers::NONE, Key::Escape),
                step,
                i.consume_key(Modifiers::NONE, Key::Tab),
                i.consume_key(Modifiers::NONE, Key::Enter),
            )
        });
        if escape {
            return (None, true);
        }
        if step != 0 && !self.results.is_empty() {
            let last = self.results.len() as i64 - 1;
            self.selected = (self.selected as i64 + step).clamp(0, last) as usize;
            self.scroll_to_selected = true;
        }
        if tab && let Some(e) = self.results.get(self.selected).filter(|e| !e.id.starts_with("calc:")) {
            self.text = e.label.clone();
            self.fresh_cursor(ui.ctx());
        }
        if enter && let Some(e) = self.results.get(self.selected) {
            return (Some(e.clone()), false);
        }

        // the head: the mark, the search box and the count, with a short accent tick under its left end
        let head = Rect::from_min_size(bx.min + vec2(1.0, 1.0), vec2(bx.width() - 2.0, HEAD_H));
        ui.painter().hline(head.x_range(), head.max.y - 0.5, Stroke::new(1.0, p.border));
        paint::fill(
            ui.painter(),
            Rect::from_min_size(pos2(head.min.x, head.max.y - 1.0), vec2(56.0, 1.0)),
            0.0,
            &tokens,
            Role::Primary,
            1.0,
        );
        let mut row = ui.new_child(
            UiBuilder::new()
                .max_rect(head.shrink2(vec2(18.0, 0.0)))
                .layout(Layout::left_to_right(Align::Center)),
        );
        row.spacing_mut().item_spacing.x = 12.0;
        let (mark, _) = row.allocate_exact_size(vec2(13.0, 13.0), Sense::hover());
        paint::sparkle(row.painter(), mark.center(), 13.0, p.primary);
        let count = format!(
            "{}{} RESULT{}",
            self.total.min(MAX_SHOWN),
            if self.total > MAX_SHOWN { "+" } else { "" },
            if self.total == 1 { "" } else { "S" }
        );
        let count_galley = row
            .painter()
            .layout_job(Text::new(count).px(10.0).mono().spacing(0.06).tone(Tone::Muted).job(&tokens));
        let input_w = row.available_width() - count_galley.size().x - 12.0;
        let input_id = base_id().with("input");
        if std::mem::take(&mut self.fresh) || self.shown_for.is_none() {
            self.fresh_cursor(row.ctx());
        }
        let edit = TextEdit::singleline(&mut self.text)
            .id(input_id)
            .frame(egui::Frame::NONE)
            .font(tokens.font(16.0, Weight::Regular, false))
            .hint_text("Search everything…  type a sum, 5 km to mi, or remind me in 20m to …")
            .desired_width(input_w)
            .margin(egui::Margin::ZERO);
        let line = tokens.fs(16.0) * 1.3;
        let input_rect =
            Rect::from_min_size(pos2(row.cursor().min.x, head.center().y - line / 2.0), vec2(input_w, line));
        let response = row.put(input_rect, edit);
        if !response.has_focus() {
            response.request_focus();
        }
        // the list below already shows what was just typed
        self.refresh(extra);
        let (count_rect, _) = row.allocate_exact_size(count_galley.size(), Sense::hover());
        row.painter().galley(count_rect.min, count_galley, p.text_muted);

        // the list
        let list =
            Rect::from_min_size(pos2(bx.min.x + 7.0, head.max.y + 6.0), vec2(bx.width() - 14.0, LIST_H));
        let mut chosen = None;
        let mut list_ui = ui.new_child(UiBuilder::new().max_rect(list).layout(Layout::top_down(Align::Min)));
        let scroll_to = std::mem::take(&mut self.scroll_to_selected).then_some(self.selected);
        egui::ScrollArea::vertical()
            .id_salt(base_id().with("list"))
            .auto_shrink([false, false])
            .max_height(LIST_H)
            .show_rows(&mut list_ui, ROW_H, self.results.len(), |ui, range| {
                for i in range {
                    let (rect, response) =
                        ui.allocate_exact_size(vec2(ui.available_width(), ROW_H), Sense::click());
                    paint_row(ui, &tokens, rect, &self.results[i], i == self.selected);
                    response.widget_info(|| {
                        egui::WidgetInfo::labeled(egui::Role::ListItem, true, &self.results[i].label)
                    });
                    if scroll_to == Some(i) {
                        ui.scroll_to_rect(rect, None);
                    }
                    if response.clicked() {
                        chosen = Some(self.results[i].clone());
                    }
                }
            });

        // the foot with the keys
        let foot = Rect::from_min_max(pos2(bx.min.x + 1.0, bx.max.y - 1.0 - FOOT_H), bx.max - vec2(1.0, 1.0));
        let low = egui::CornerRadius {
            nw: 0,
            ne: 0,
            sw: (tokens.r() - 1.0).max(0.0) as u8,
            se: (tokens.r() - 1.0).max(0.0) as u8,
        };
        ui.painter().rect_filled(foot, low, p.bg_header);
        ui.painter().hline(foot.x_range(), foot.min.y + 0.5, Stroke::new(1.0, p.border));
        let mut keys = ui.new_child(
            UiBuilder::new()
                .max_rect(foot.shrink2(vec2(18.0, 0.0)))
                .layout(Layout::left_to_right(Align::Center)),
        );
        keys.spacing_mut().item_spacing.x = 18.0;
        for text in ["↑↓ MOVE", "ENTER RUN", "TAB FILL"] {
            keys.add(Text::new(text).px(10.0).mono().spacing(0.04).tone(Tone::Muted));
        }
        keys.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.add(Text::new("ESC CLOSE").px(10.0).mono().spacing(0.04).tone(Tone::Muted));
        });
        (chosen, false)
    }

    /// Puts the text cursor at the end of what is in the box (it was typed on the overlay, or Tab filled it).
    fn fresh_cursor(&self, ctx: &Context) {
        let id = base_id().with("input");
        let mut state = egui::text_edit::TextEditState::load(ctx, id).unwrap_or_default();
        let end = CCursor::new(self.text.chars().count());
        state.cursor.set_char_range(Some(CCursorRange::one(end)));
        state.store(ctx, id);
    }
}

fn paint_row(ui: &Ui, tokens: &Tokens, rect: Rect, entry: &Entry, selected: bool) {
    let p = &tokens.p;
    let painter = ui.painter();
    if selected {
        painter.rect_filled(rect, (tokens.r() - 4.0).max(0.0), p.primary_muted);
        let bar = Rect::from_min_size(pos2(rect.min.x, rect.min.y + 10.0), vec2(2.0, rect.height() - 20.0));
        paint::fill(painter, bar, 1.0, tokens, Role::Primary, 1.0);
    }
    let (group, keys) = entry.columns();
    let x = rect.min.x + 14.0;
    let elided = |text: Text, width: f32| {
        let mut job = text.job(tokens);
        job.wrap.max_width = width;
        job.wrap.max_rows = 1;
        job.wrap.break_anywhere = true;
        painter.layout_job(job)
    };
    let group =
        elided(Text::new(group).px(9.0).medium().mono().spacing(0.08).tone(Tone::Muted), GROUP_W - 6.0);
    painter.galley(pos2(x, rect.center().y - group.size().y / 2.0), group, p.text_muted);
    let mut right = 0.0;
    if !keys.is_empty() {
        let keys = elided(Text::new(keys).px(11.0).mono().tone(Tone::Muted), 170.0);
        right = keys.size().x + 4.0;
        painter.galley(
            pos2(rect.max.x - 14.0 - keys.size().x, rect.center().y - keys.size().y / 2.0),
            keys,
            p.text_muted,
        );
    }
    let lx = x + GROUP_W + 12.0;
    let avail = (rect.max.x - 14.0 - right - 16.0 - lx).max(10.0);
    let label = elided(Text::new(&entry.label).px(13.5), avail);
    painter.galley(pos2(lx, rect.center().y - label.size().y / 2.0), label, p.text_primary);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn index() -> Vec<Entry> {
        vec![
            Entry::new("page:dashboard", "Open Dashboard", "Luminescent", "page"),
            Entry::new("page:themes", "Open Themes", "Luminescent", "page"),
            Entry::new("act:timer:stopwatch", "New stopwatch", "Timer / Stopwatch · Ctrl+Alt+T", "action"),
            Entry::new("theme:dark", "Theme: Dark", "active", "theme"),
            Entry::new("settings:timer", "Settings › Timer / Stopwatch", "settings", "settings"),
            Entry::new("lum:Restart Luminescent", "Restart Luminescent", "Luminescent", "luminescent"),
        ]
    }

    #[test]
    fn groups_and_keys_like_the_python_rows() {
        let e = index();
        assert_eq!(e[2].columns(), ("TIMER / STOPWATCH".into(), "Ctrl+Alt+T".into()));
        assert_eq!(e[3].columns(), ("THEME".into(), "ACTIVE".into()));
        assert_eq!(e[0].columns(), ("OVERLAY".into(), String::new()));
        assert_eq!(e[4].columns(), ("SETTINGS".into(), String::new()));
        assert_eq!(
            Entry::new("calc:result", "= 4", "2+2 · Enter copies", "calc").columns().1,
            "ENTER COPIES"
        );
    }

    #[test]
    fn an_empty_query_puts_the_recent_first() {
        let mut search = Search::new(index());
        let (found, total) = search.results("", &["theme:dark".into(), "gone:away".into()]);
        assert_eq!(total, 6);
        assert_eq!(found[0].id, "theme:dark");
        assert_eq!(found.len(), 6);
    }

    #[test]
    fn fuzzy_matches_rank_the_best_first() {
        let mut search = Search::new(index());
        let (found, _) = search.results("stopw", &[]);
        assert_eq!(found[0].id, "act:timer:stopwatch");
        let (found, total) = search.results("zzqx", &[]);
        assert!(found.is_empty() && total == 0);
        let (found, _) = search.results("open th", &[]);
        assert_eq!(found[0].id, "page:themes");
    }

    #[test]
    fn two_thousand_entries_search_within_the_budget() {
        let entries: Vec<Entry> = (0..2000)
            .map(|i| {
                Entry::new(
                    format!("item:{i}"),
                    format!("Item number {i} of the dashboard"),
                    "Work · Alt+1",
                    "item",
                )
            })
            .collect();
        let mut search = Search::new(entries);
        let started = std::time::Instant::now();
        let (found, _) = search.results("num 19", &[]);
        let ms = started.elapsed().as_secs_f64() * 1000.0;
        assert!(!found.is_empty());
        // PERFORMANCE.md: 5 ms a keystroke; a debug build is several times slower
        let budget = if cfg!(debug_assertions) { 60.0 } else { 5.0 };
        assert!(ms < budget, "{ms:.2} ms");
    }
}
