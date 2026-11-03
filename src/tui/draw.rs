//! Renders the shared display model with ratatui.

use std::time::Duration;

use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Color, Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph, Sparkline};
use unicode_width::UnicodeWidthChar;

use crate::display::{self, Level, Panel, Widget};
use crate::fmt;
use crate::host::Host;
use crate::layout;

/// Narrowest column before the layout drops to fewer columns.
const MIN_COLUMN_WIDTH: u16 = 56;
const MAX_LABEL_WIDTH: usize = 20;
/// Fits the widest formatted value, e.g. `1023.99 KiB/s`.
const VALUE_WIDTH: u16 = 13;
/// Sparkline resolution; heights are fractions scaled to this before drawing.
const SPARK_STEPS: u64 = 1000;

pub fn frame(f: &mut Frame, host: &Host, panels: &[Panel], interval: Duration) {
    let [main, footer] =
        Layout::vertical([Constraint::Fill(1), Constraint::Length(1)]).areas(f.area());

    let count = usize::from(main.width / MIN_COLUMN_WIDTH).max(1);
    let columns = layout::columns(panels, count, layout::rows);
    let rects = Layout::horizontal(vec![Constraint::Fill(1); columns.len()]).split(main);
    let mut hidden = 0;
    for (column, rect) in columns.iter().zip(rects.iter()) {
        // Stacked top-down in priority order, so what doesn't fit is the least important.
        let mut y = rect.y;
        for &i in column {
            let left = rect.bottom().saturating_sub(y);
            // A border and one row is the least worth drawing.
            if left < 3 {
                hidden += 1;
                continue;
            }
            let rows = u16::try_from(layout::rows(&panels[i])).unwrap_or(u16::MAX);
            let height = rows.min(left);
            draw_panel(f, host, &panels[i], Rect { y, height, ..*rect });
            y += height;
        }
    }

    let mut help = format!(" q quit · sampling every {}", fmt::interval(interval));
    if hidden > 0 {
        help.push_str(&format!(" · {hidden} hidden, enlarge the terminal"));
    }
    f.render_widget(Paragraph::new(help).dark_gray(), footer);
}

fn draw_panel(f: &mut Frame, host: &Host, panel: &Panel, area: Rect) {
    let block = Block::bordered().title(format!(" {} ", panel.title).bold());
    let inner = block.inner(area);
    f.render_widget(block, area);

    let label_width = panel
        .widgets
        .iter()
        .map(|w| Span::raw(w.label()).width())
        .max()
        .unwrap_or(0)
        .min(MAX_LABEL_WIDTH);
    // When rows don't all fit, the last line says how many were left out.
    let room = usize::from(inner.height);
    let total = panel.widgets.len();
    let shown = if total > room {
        room.saturating_sub(1)
    } else {
        total
    };
    for (k, widget) in panel.widgets.iter().take(shown).enumerate() {
        let row = Rect {
            y: inner.y + k as u16,
            height: 1,
            ..inner
        };
        let [label, body, value] = Layout::horizontal([
            Constraint::Length(label_width as u16),
            Constraint::Fill(1),
            Constraint::Length(VALUE_WIDTH),
        ])
        .spacing(1)
        .areas(row);
        draw_row(f, host, widget, label_width, [label, body, value]);
    }
    if shown < total && room > 0 {
        let row = Rect {
            y: inner.y + shown as u16,
            height: 1,
            ..inner
        };
        let more = format!("+{} more", total - shown);
        f.render_widget(Paragraph::new(more).dark_gray(), row);
    }
}

fn draw_row(
    f: &mut Frame,
    host: &Host,
    widget: &Widget,
    label_width: usize,
    [label, body, value]: [Rect; 3],
) {
    let store = host.store();
    let unit = store.desc(widget.metric()).map(|d| d.unit);
    let latest = store.latest(widget.metric());

    f.render_widget(Paragraph::new(fit(widget.label(), label_width)), label);
    let text = fmt::reading(latest, unit);
    f.render_widget(Paragraph::new(text).alignment(Alignment::Right), value);

    match widget {
        Widget::Gauge { .. } => {
            // Only units with a fixed full scale get a bar; other gauges show just the value.
            if let Some(ratio) = latest.zip(unit).and_then(|(v, u)| display::ratio(v, u)) {
                f.render_widget(bar(ratio, widget.level(ratio), body.width), body);
            }
        }
        Widget::Series { .. } => {
            let history: Vec<_> = store.history(widget.metric()).collect();
            let recent = &history[history.len().saturating_sub(usize::from(body.width))..];
            // Right-aligned, so the newest sample is always at the right edge.
            let width = recent.len() as u16;
            let area = Rect {
                x: body.right() - width,
                width,
                ..body
            };
            let heights = display::spark_heights(recent, unit);
            let spark = Sparkline::default()
                .data(heights.iter().map(|h| h.map(steps)))
                .max(SPARK_STEPS)
                .style(Style::new().fg(Color::Cyan))
                // A gap must not look like a zero, which draws as blank.
                .absent_value_symbol("·")
                .absent_value_style(Style::new().dark_gray());
            f.render_widget(spark, area);
        }
    }
}

fn steps(height: f64) -> u64 {
    (height * SPARK_STEPS as f64).round() as u64
}

/// `label` in at most `width` columns. Long ones keep their end ("…ディスク used"),
/// which tells rows of the same device apart.
fn fit(label: &str, width: usize) -> String {
    if Span::raw(label).width() <= width {
        return label.to_string();
    }
    let mut tail = Vec::new();
    let mut used = 1; // the ellipsis
    for c in label.chars().rev() {
        let w = c.width().unwrap_or(0);
        if used + w > width {
            break;
        }
        used += w;
        tail.push(c);
    }
    std::iter::once('…').chain(tail.into_iter().rev()).collect()
}

/// Thick filled part, thin dim remainder: readable with or without color.
fn bar(ratio: f64, level: Level, width: u16) -> Line<'static> {
    let filled = (ratio * f64::from(width)).round() as usize;
    let rest = usize::from(width).saturating_sub(filled);
    Line::from(vec![
        Span::styled("━".repeat(filled), Style::new().fg(level_color(level))),
        Span::styled("─".repeat(rest), Style::new().dark_gray()),
    ])
}

fn level_color(level: Level) -> Color {
    match level {
        Level::Critical => Color::Red,
        Level::High => Color::Yellow,
        Level::Normal => Color::Green,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metric::{Availability, Capabilities, MetricDesc, MetricId, Unit};
    use crate::module::{Ctx, Header, Module, Sink};
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    const LOAD: MetricId = MetricId::new("test.load");
    const RATE: MetricId = MetricId::new("test.rate");

    /// Pushes `values[tick]` for RATE (None leaves a gap) and 42% LOAD every tick.
    struct Fake {
        tick: usize,
        values: Vec<Option<f64>>,
        panels: Vec<Panel>,
    }

    impl Module for Fake {
        fn header(&self) -> &'static Header {
            &Header {
                id: "test",
                name: "Test",
                reads: "nothing; stand-in",
            }
        }

        fn probe(&mut self, _ctx: &Ctx) -> Capabilities {
            Capabilities::all([LOAD, RATE], Availability::Available)
        }

        fn metrics(&self) -> Vec<MetricDesc> {
            vec![
                MetricDesc::gauge(LOAD, Unit::Percent),
                MetricDesc::counter(RATE, Unit::BytesPerSec),
            ]
        }

        fn sample(&mut self, _ctx: &Ctx, out: &mut Sink) {
            out.push(LOAD, 42.0);
            if let Some(Some(v)) = self.values.get(self.tick) {
                out.push(RATE, *v);
            }
            self.tick += 1;
        }

        fn panels(&self) -> Vec<Panel> {
            self.panels.clone()
        }
    }

    fn standard_panel(title: &str) -> Panel {
        Panel::new("test", title)
            .widget(Widget::gauge("load", LOAD))
            .widget(Widget::series("rate", RATE))
    }

    /// Screen text, one string per row.
    fn render(fake: Fake, ticks: usize, width: u16, height: u16) -> Vec<String> {
        let mut host = Host::new(vec![Box::new(fake)]);
        for _ in 0..ticks {
            host.tick();
        }
        let panels = host.panels();
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal
            .draw(|f| frame(f, &host, &panels, Duration::from_millis(500)))
            .unwrap();
        let buffer = terminal.backend().buffer();
        (0..height)
            .map(|y| (0..width).map(|x| buffer[(x, y)].symbol()).collect())
            .collect()
    }

    fn fake(values: Vec<Option<f64>>, panels: Vec<Panel>) -> Fake {
        Fake {
            tick: 0,
            values,
            panels,
        }
    }

    #[test]
    fn draws_panels_and_values() {
        let screen = render(fake(vec![None], vec![standard_panel("Fake")]), 1, 80, 10).concat();
        assert!(screen.contains(" Fake "));
        assert!(screen.contains("load"));
        assert!(screen.contains("42.0%"));
        assert!(screen.contains("━") && screen.contains("─"));
        // No RATE value this tick.
        assert!(screen.contains("–"));
        assert!(screen.contains("sampling every 500ms"));
    }

    #[test]
    fn survives_tiny_terminals() {
        render(fake(vec![], vec![standard_panel("Fake")]), 1, 10, 3);
        render(fake(vec![], vec![standard_panel("Fake")]), 1, 1, 1);
    }

    #[test]
    fn gaps_differ_from_zeros_and_newest_is_rightmost() {
        let values = vec![Some(0.0), None, Some(5.0)];
        let screen = render(fake(values, vec![standard_panel("Fake")]), 3, 60, 6);
        let row = screen
            .iter()
            .find(|r| r.contains("rate"))
            .cloned()
            .unwrap_or_default();
        let body: Vec<char> = row.chars().collect();
        let value_start = body.len() - 1 - usize::from(VALUE_WIDTH);
        // The last three body cells: zero (blank), gap (·), full bar.
        let cells: String = body[value_start - 4..value_start - 1].iter().collect();
        assert_eq!(cells, " ·█", "{row}");
    }

    #[test]
    fn control_characters_never_reach_the_screen() {
        let evil = Panel::new("test", "\x1b[2JEvil")
            .widget(Widget::series("\x1b]52;c;cm0=\x07disk", RATE));
        let screen = render(fake(vec![Some(1.0)], vec![evil]), 1, 60, 6).concat();
        assert!(!screen.chars().any(char::is_control), "{screen:?}");
        assert!(screen.contains("Evil") && screen.contains("disk"));
    }

    #[test]
    fn tall_panels_clip_and_say_so() {
        let tall = (0..17).fold(Panel::new("test", "Tall").priority(90), |p, i| {
            p.widget(Widget::gauge(format!("row {i}"), LOAD))
        });
        let small = |t: &str| standard_panel(t).priority(10);
        let panels = vec![tall, small("A"), small("B"), small("C"), small("D")];
        let screen = render(fake(vec![], panels), 1, 50, 12);
        let text = screen.concat();
        // The top rows survive, the rest are counted, and hidden panels are reported.
        assert!(text.contains("row 0"));
        assert!(text.contains("more"), "{}", screen.join("\n"));
        assert!(text.contains("hidden"), "{}", screen.join("\n"));
    }

    #[test]
    fn long_labels_keep_their_end() {
        assert_eq!(fit("short", 10), "short");
        assert_eq!(fit("Macintosh HD Backup read", 12), "…Backup read");
        let wide = fit("外部ディスク外部ディスク used", 10);
        assert!(
            Span::raw(wide.as_str()).width() <= 10 && wide.ends_with("used"),
            "{wide}"
        );
    }
}
