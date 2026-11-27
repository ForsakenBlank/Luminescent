//! The 👁 preview: Markdown parsed into a small tree of blocks and drawn with egui. It reads CommonMark with
//! tables, task lists and strikethrough, which is what Qt's `setMarkdown` understood in the Python app.

use egui::text::{LayoutJob, TextFormat};
use egui::{Color32, Label, Rect, Sense, Stroke, Ui, pos2, vec2};
use pulldown_cmark::{Event, HeadingLevel, Options, Parser, Tag, TagEnd};

use crate::fonts::Weight;
use crate::theme::Tokens;

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Span {
    pub text: String,
    pub bold: bool,
    pub italic: bool,
    pub strike: bool,
    pub code: bool,
    pub link: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ListItem {
    /// `Some(done)` for a `- [x]` task.
    pub task: Option<bool>,
    pub blocks: Vec<Block>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Block {
    Heading(u8, Vec<Span>),
    Paragraph(Vec<Span>),
    Quote(Vec<Block>),
    List { first: Option<u64>, items: Vec<ListItem> },
    Code(String),
    Rule,
    Table { head: Vec<Vec<Span>>, rows: Vec<Vec<Vec<Span>>> },
}

pub fn parse(text: &str) -> Vec<Block> {
    let options = Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS;
    let events: Vec<Event> = Parser::new_ext(text, options).collect();
    let mut at = 0;
    blocks(&events, &mut at)
}

fn is_inline(event: &Event) -> bool {
    match event {
        Event::Text(_)
        | Event::Code(_)
        | Event::SoftBreak
        | Event::HardBreak
        | Event::InlineHtml(_)
        | Event::InlineMath(_)
        | Event::FootnoteReference(_) => true,
        Event::Start(tag) => {
            matches!(
                tag,
                Tag::Emphasis | Tag::Strong | Tag::Strikethrough | Tag::Link { .. } | Tag::Image { .. }
            )
        }
        _ => false,
    }
}

/// Text with its styling, up to the event that is not inline (that one is left for the caller).
fn inline_run(events: &[Event], at: &mut usize) -> Vec<Span> {
    let mut spans: Vec<Span> = Vec::new();
    let (mut bold, mut italic, mut strike) = (0, 0, 0);
    let mut links: Vec<String> = Vec::new();
    let mut push = |text: &str, code: bool, links: &[String], flags: (i32, i32, i32)| {
        spans.push(Span {
            text: text.to_owned(),
            bold: flags.0 > 0,
            italic: flags.1 > 0,
            strike: flags.2 > 0,
            code,
            link: links.last().cloned(),
        });
    };
    while let Some(event) = events.get(*at) {
        match event {
            Event::Text(t) | Event::InlineHtml(t) | Event::InlineMath(t) => {
                push(t, false, &links, (bold, italic, strike));
            }
            Event::Code(t) => push(t, true, &links, (bold, italic, strike)),
            Event::SoftBreak => push(" ", false, &links, (bold, italic, strike)),
            Event::HardBreak => push("\n", false, &links, (bold, italic, strike)),
            Event::FootnoteReference(t) => push(&format!("[{t}]"), false, &links, (bold, italic, strike)),
            Event::Start(Tag::Strong) => bold += 1,
            Event::Start(Tag::Emphasis) => italic += 1,
            Event::Start(Tag::Strikethrough) => strike += 1,
            Event::Start(Tag::Link { dest_url, .. }) => links.push(dest_url.to_string()),
            // a picture can not be shown here: its description stands in for it
            Event::Start(Tag::Image { .. }) => push("[image] ", false, &links, (bold, italic, strike)),
            Event::End(TagEnd::Strong) => bold -= 1,
            Event::End(TagEnd::Emphasis) => italic -= 1,
            Event::End(TagEnd::Strikethrough) => strike -= 1,
            Event::End(TagEnd::Link) => {
                links.pop();
            }
            Event::End(TagEnd::Image) => {}
            _ => break,
        }
        *at += 1;
    }
    spans
}

fn plain_text(events: &[Event], at: &mut usize, until: TagEnd) -> String {
    let mut out = String::new();
    while let Some(event) = events.get(*at) {
        *at += 1;
        match event {
            Event::End(tag) if *tag == until => break,
            Event::Text(t) | Event::Code(t) | Event::Html(t) => out.push_str(t),
            Event::SoftBreak | Event::HardBreak => out.push('\n'),
            _ => {}
        }
    }
    out
}

/// Blocks up to the end tag of the container they are in (which is consumed), or the end of the text.
fn blocks(events: &[Event], at: &mut usize) -> Vec<Block> {
    let mut out = Vec::new();
    while let Some(event) = events.get(*at) {
        match event {
            Event::End(_) => {
                *at += 1;
                break;
            }
            Event::Start(Tag::Paragraph) => {
                *at += 1;
                let spans = inline_run(events, at);
                skip_end(events, at, TagEnd::Paragraph);
                out.push(Block::Paragraph(spans));
            }
            Event::Start(Tag::Heading { level, .. }) => {
                let level = heading_number(*level);
                *at += 1;
                let spans = inline_run(events, at);
                skip_end(events, at, TagEnd::Heading(HeadingLevel::H1));
                out.push(Block::Heading(level, spans));
            }
            Event::Start(Tag::BlockQuote(_)) => {
                *at += 1;
                out.push(Block::Quote(blocks(events, at)));
            }
            Event::Start(Tag::CodeBlock(_)) => {
                *at += 1;
                let text = plain_text(events, at, TagEnd::CodeBlock);
                out.push(Block::Code(text.trim_end_matches('\n').to_owned()));
            }
            Event::Start(Tag::List(first)) => {
                *at += 1;
                out.push(list(events, at, *first));
            }
            Event::Start(Tag::Table(_)) => {
                *at += 1;
                out.push(table(events, at));
            }
            Event::Rule => {
                *at += 1;
                out.push(Block::Rule);
            }
            Event::Start(Tag::HtmlBlock) => {
                *at += 1;
                let text = plain_text(events, at, TagEnd::HtmlBlock);
                out.push(Block::Paragraph(vec![Span { text: text.trim().to_owned(), ..Span::default() }]));
            }
            e if is_inline(e) => {
                // a tight list item holds its text with no paragraph around it
                let spans = inline_run(events, at);
                out.push(Block::Paragraph(spans));
            }
            _ => *at += 1,
        }
    }
    out
}

fn skip_end(events: &[Event], at: &mut usize, wanted: TagEnd) {
    if let Some(Event::End(tag)) = events.get(*at) {
        let same_kind = std::mem::discriminant(tag) == std::mem::discriminant(&wanted);
        if same_kind {
            *at += 1;
        }
    }
}

fn heading_number(level: HeadingLevel) -> u8 {
    match level {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
}

fn list(events: &[Event], at: &mut usize, first: Option<u64>) -> Block {
    let mut items = Vec::new();
    while let Some(event) = events.get(*at) {
        match event {
            Event::Start(Tag::Item) => {
                *at += 1;
                let task = match events.get(*at) {
                    Some(Event::TaskListMarker(done)) => {
                        *at += 1;
                        Some(*done)
                    }
                    _ => None,
                };
                items.push(ListItem { task, blocks: blocks(events, at) });
            }
            Event::End(TagEnd::List(_)) => {
                *at += 1;
                break;
            }
            _ => *at += 1,
        }
    }
    Block::List { first, items }
}

fn cells(events: &[Event], at: &mut usize, until: TagEnd) -> Vec<Vec<Span>> {
    let mut out = Vec::new();
    while let Some(event) = events.get(*at) {
        match event {
            Event::Start(Tag::TableCell) => {
                *at += 1;
                out.push(inline_run(events, at));
                skip_end(events, at, TagEnd::TableCell);
            }
            Event::End(tag) if *tag == until => {
                *at += 1;
                break;
            }
            _ => *at += 1,
        }
    }
    out
}

fn table(events: &[Event], at: &mut usize) -> Block {
    let (mut head, mut rows) = (Vec::new(), Vec::new());
    while let Some(event) = events.get(*at) {
        match event {
            Event::Start(Tag::TableHead) => {
                *at += 1;
                head = cells(events, at, TagEnd::TableHead);
            }
            Event::Start(Tag::TableRow) => {
                *at += 1;
                rows.push(cells(events, at, TagEnd::TableRow));
            }
            Event::End(TagEnd::Table) => {
                *at += 1;
                break;
            }
            _ => *at += 1,
        }
    }
    Block::Table { head, rows }
}

/// Where a clicked link may lead: web pages and mail, nothing that would open a file or run something.
pub fn safe_link(url: &str) -> bool {
    let lower = url.trim().to_ascii_lowercase();
    lower.starts_with("http://") || lower.starts_with("https://") || lower.starts_with("mailto:")
}

struct Look<'a> {
    tokens: &'a Tokens,
    px: f32,
}

/// Draws the preview into `ui`. Gives back a link that was clicked, if it is safe to open.
pub fn show(ui: &mut Ui, text: &str, px: f32) -> Option<String> {
    let tokens = Tokens::of(ui.ctx());
    let tree = parse(text);
    let look = Look { tokens: &tokens, px };
    let mut clicked = None;
    ui.spacing_mut().item_spacing.y = 8.0;
    for block in &tree {
        draw_block(ui, block, &look, &mut clicked);
    }
    clicked.filter(|url| safe_link(url))
}

fn draw_block(ui: &mut Ui, block: &Block, look: &Look<'_>, clicked: &mut Option<String>) {
    let (tokens, px) = (look.tokens, look.px);
    let p = &tokens.p;
    match block {
        Block::Heading(level, spans) => {
            let scale = match level {
                1 => 1.6,
                2 => 1.35,
                3 => 1.18,
                _ => 1.05,
            };
            ui.add_space(2.0);
            spans_row(ui, spans, look, px * scale, Some(Weight::SemiBold), clicked);
        }
        Block::Paragraph(spans) => spans_row(ui, spans, look, px, None, clicked),
        Block::Quote(inner) => {
            let top = ui.cursor().min;
            let response = ui.scope(|ui| {
                ui.add_space(0.0);
                ui.horizontal(|ui| {
                    ui.add_space(14.0);
                    ui.vertical(|ui| {
                        for b in inner {
                            draw_block(ui, b, look, clicked);
                        }
                    });
                });
            });
            let bar =
                Rect::from_min_max(pos2(top.x + 2.0, top.y), pos2(top.x + 5.0, response.response.rect.max.y));
            ui.painter().rect_filled(bar, 1.5, p.primary);
        }
        Block::List { first, items } => {
            for (n, item) in items.iter().enumerate() {
                ui.horizontal_top(|ui| {
                    ui.spacing_mut().item_spacing.x = 6.0;
                    let (gutter, _) = ui.allocate_exact_size(vec2(24.0, tokens.fs(px) * 1.3), Sense::hover());
                    match (item.task, first) {
                        (Some(done), _) => paint_task(ui, gutter, done, tokens),
                        (None, Some(start)) => {
                            ui.painter().text(
                                gutter.right_center(),
                                egui::Align2::RIGHT_CENTER,
                                format!("{}.", start + n as u64),
                                tokens.font(px, Weight::Regular, false),
                                p.text_secondary,
                            );
                        }
                        (None, None) => {
                            ui.painter().circle_filled(
                                pos2(gutter.right() - 5.0, gutter.center().y),
                                2.2,
                                p.text_secondary,
                            );
                        }
                    }
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = 4.0;
                        for b in &item.blocks {
                            draw_block(ui, b, look, clicked);
                        }
                    });
                });
            }
        }
        Block::Code(text) => {
            egui::Frame::new().fill(p.bg_card).corner_radius(tokens.rs()).inner_margin(10).show(ui, |ui| {
                ui.set_width(ui.available_width());
                let font = tokens.font(px * 0.9, Weight::Regular, true);
                ui.add(
                    Label::new(egui::RichText::new(text).font(font).color(p.text_primary)).selectable(true),
                );
            });
        }
        Block::Rule => {
            let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 6.0), Sense::hover());
            ui.painter().hline(rect.x_range(), rect.center().y, Stroke::new(1.0, p.border));
        }
        Block::Table { head, rows } => {
            egui::Grid::new(ui.next_auto_id()).spacing(vec2(16.0, 6.0)).show(ui, |ui| {
                for cell in head {
                    spans_row(ui, cell, look, px, Some(Weight::SemiBold), clicked);
                }
                ui.end_row();
                for row in rows {
                    for cell in row {
                        spans_row(ui, cell, look, px, None, clicked);
                    }
                    ui.end_row();
                }
            });
        }
    }
}

fn paint_task(ui: &Ui, gutter: Rect, done: bool, tokens: &Tokens) {
    let p = &tokens.p;
    let size = 13.0;
    let rect = Rect::from_center_size(pos2(gutter.right() - 7.0, gutter.center().y), vec2(size, size));
    if done {
        ui.painter().rect_filled(rect, 3.0, p.primary);
        let at = |x: f32, y: f32| rect.min + vec2(x, y) * size;
        ui.painter().add(egui::Shape::line(
            vec![at(0.24, 0.52), at(0.42, 0.7), at(0.76, 0.32)],
            Stroke::new(1.5, p.on_primary),
        ));
    } else {
        ui.painter().rect_stroke(rect, 3.0, Stroke::new(1.5, p.text_muted), egui::StrokeKind::Inside);
    }
}

/// The text of one paragraph, heading or table cell. Spans flow on from one another and wrap together.
fn spans_row(
    ui: &mut Ui,
    spans: &[Span],
    look: &Look<'_>,
    px: f32,
    weight: Option<Weight>,
    clicked: &mut Option<String>,
) {
    let tokens = look.tokens;
    let p = &tokens.p;
    // a row of text is as tall as its text, not as a button: the height has to be set before the row starts
    ui.scope(|ui| {
        ui.spacing_mut().interact_size.y = tokens.fs(px) * 1.2;
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = vec2(0.0, 2.0);
            for span in spans {
                let weight = if span.bold { Weight::SemiBold } else { weight.unwrap_or(Weight::Regular) };
                let colour: Color32 = if span.link.is_some() { p.primary_text } else { p.text_primary };
                let format = TextFormat {
                    font_id: tokens.font(if span.code { px * 0.92 } else { px }, weight, span.code),
                    color: colour,
                    italics: span.italic,
                    strikethrough: if span.strike { Stroke::new(1.0, colour) } else { Stroke::NONE },
                    underline: if span.link.is_some() { Stroke::new(1.0, colour) } else { Stroke::NONE },
                    background: if span.code { p.bg_card } else { Color32::TRANSPARENT },
                    ..Default::default()
                };
                let job = LayoutJob::single_section(span.text.clone(), format);
                match &span.link {
                    Some(url) => {
                        let response = ui.add(Label::new(job).sense(Sense::click()));
                        if response.hovered() {
                            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                        }
                        if response.on_hover_text(url).clicked() {
                            *clicked = Some(url.clone());
                        }
                    }
                    None => {
                        ui.add(Label::new(job).selectable(true));
                    }
                }
            }
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(spans: &[Span]) -> String {
        spans.iter().map(|s| s.text.as_str()).collect()
    }

    #[test]
    fn headings_and_emphasis_are_read() {
        let tree = parse("# Ideas\n\nTry the **Rust** build, *really*, ~~not~~ `now`.");
        assert_eq!(tree.len(), 2);
        let Block::Heading(1, title) = &tree[0] else { panic!("a heading first") };
        assert_eq!(text(title), "Ideas");
        let Block::Paragraph(spans) = &tree[1] else { panic!("then a paragraph") };
        assert_eq!(text(spans), "Try the Rust build, really, not now.");
        assert!(spans.iter().any(|s| s.text == "Rust" && s.bold));
        assert!(spans.iter().any(|s| s.text == "really" && s.italic));
        assert!(spans.iter().any(|s| s.text == "not" && s.strike));
        assert!(spans.iter().any(|s| s.text == "now" && s.code));
    }

    #[test]
    fn lists_nest_and_tasks_know_if_they_are_done() {
        let tree = parse("- one\n- [x] two\n  - inner\n\n1. first\n2. second");
        let Block::List { first: None, items } = &tree[0] else { panic!("a bullet list") };
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].task, None);
        assert_eq!(items[1].task, Some(true));
        let Block::List { first: None, items: inner } = &items[1].blocks[1] else { panic!("a nested list") };
        assert_eq!(inner.len(), 1);
        let Block::List { first: Some(1), items: numbered } = &tree[1] else { panic!("a numbered list") };
        assert_eq!(numbered.len(), 2);
    }

    #[test]
    fn links_quotes_code_rules_and_tables_are_read() {
        let tree = parse(
            "[home](https://example.com)\n\n> quoted\n\n```\nlet x = 1;\n```\n\n---\n\n| a | b |\n|---|---|\n| 1 | 2 |",
        );
        let Block::Paragraph(spans) = &tree[0] else { panic!("a paragraph") };
        assert_eq!(spans[0].link.as_deref(), Some("https://example.com"));
        assert!(matches!(&tree[1], Block::Quote(inner) if matches!(&inner[0], Block::Paragraph(_))));
        assert_eq!(tree[2], Block::Code("let x = 1;".into()));
        assert_eq!(tree[3], Block::Rule);
        let Block::Table { head, rows } = &tree[4] else { panic!("a table") };
        assert_eq!((head.len(), rows.len(), rows[0].len()), (2, 1, 2));
        assert_eq!(text(&rows[0][1]), "2");
    }

    #[test]
    fn plain_text_and_the_empty_note_come_out_without_trouble() {
        assert!(parse("").is_empty());
        let tree = parse("just words\nsecond line");
        assert_eq!(tree.len(), 1);
        let Block::Paragraph(spans) = &tree[0] else { panic!("a paragraph") };
        assert_eq!(text(spans), "just words second line");
    }

    #[test]
    fn only_web_and_mail_links_may_be_opened() {
        assert!(safe_link("https://example.com/a"));
        assert!(safe_link("HTTP://example.com"));
        assert!(safe_link("mailto:me@example.com"));
        assert!(!safe_link("file:///etc/passwd"));
        assert!(!safe_link("/etc/passwd"));
        assert!(!safe_link("javascript:alert(1)"));
    }
}
