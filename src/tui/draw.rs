//! Renders the shared display model with ratatui.

use std::time::Duration;

use crate::display::{Level, Panel, Widget, full_scale};
use crate::fmt;
use crate::host::Host;
use crate::layout;
use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Color, Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph, Sparkline};

/// Narrowest column before the layout drops to fewer columns.
const MIN_COLUMN_WIDTH: u16 = 56;
const MAX_LABEL_WIDTH: usize = 20;
const VALUE_WIDTH: u16 = 12;

pub fn frame(f: &mut Frame, host: &Host, panels: &[Panel], interval: Duration) {
    let [main, footer] =
        Layout::vertical([Constraint::Fill(1), Constraint::Length(1)]).areas(f.area());

    let count = usize::from(main.width / MIN_COLUMN_WIDTH).max(1);
    let columns = layout::columns(panels, count, |p| usize::from(height(p)));
    let rects = Layout::horizontal(vec![Constraint::Fill(1); columns.len()]).split(main);
    for (column, rect) in columns.iter().zip(rects.iter()) {
        let heights = column
            .iter()
            .map(|&i| Constraint::Length(height(&panels[i])));
        let areas = Layout::vertical(heights).split(*rect);
        for (&i, area) in column.iter().zip(areas.iter()) {
            draw_panel(f, host, &panels[i], *area);
        }
    }

    let help = format!(" q quit · sampling every {}", fmt::interval(interval));
    f.render_widget(Paragraph::new(help).dark_gray(), footer);
}

/// Rows a panel needs: one per widget plus its border.
fn height(panel: &Panel) -> u16 {
    u16::try_from(panel.widgets.len() + 2).unwrap_or(u16::MAX)
}

fn draw_panel(f: &mut Frame, host: &Host, panel: &Panel, area: Rect) {
    let block = Block::bordered().title(format!(" {} ", panel.title).bold());
    let inner = block.inner(area);
    f.render_widget(block, area);

    let label_width = panel
        .widgets
        .iter()
        .map(|w| w.label().chars().count())
        .max()
        .unwrap_or(0)
        .min(MAX_LABEL_WIDTH);
    let label_width = u16::try_from(label_width).unwrap_or(0);
    let rows = Layout::vertical(vec![Constraint::Length(1); panel.widgets.len()]).split(inner);
    for (widget, row) in panel.widgets.iter().zip(rows.iter()) {
        let [label, body, value] = Layout::horizontal([
            Constraint::Length(label_width),
            Constraint::Fill(1),
            Constraint::Length(VALUE_WIDTH),
        ])
        .spacing(1)
        .areas(*row);
        draw_row(f, host, widget, [label, body, value]);
    }
}

fn draw_row(f: &mut Frame, host: &Host, widget: &Widget, [label, body, value]: [Rect; 3]) {
    let store = host.store();
    let unit = store.desc(widget.metric()).map(|d| d.unit);
    let latest = store.latest(widget.metric());

    f.render_widget(Paragraph::new(widget.label()), label);
    let text = latest
        .zip(unit)
        .map_or_else(|| "–".into(), |(v, u)| fmt::value(v, u));
    f.render_widget(Paragraph::new(text).alignment(Alignment::Right), value);

    match widget {
        Widget::Gauge { .. } => {
            // Only units with a fixed full scale get a bar; other gauges show just the value.
            if let (Some(v), Some(full)) = (latest, unit.and_then(full_scale)) {
                f.render_widget(bar((v / full).clamp(0.0, 1.0), body.width), body);
            }
        }
        Widget::Series { .. } => {
            let history: Vec<_> = store.history(widget.metric()).collect();
            let shown = &history[history.len().saturating_sub(usize::from(body.width))..];
            let mut spark = Sparkline::default()
                .data(shown.iter().map(|v| v.map(spark_value)))
                .style(Style::new().fg(Color::Cyan));
            if let Some(full) = unit.and_then(full_scale) {
                spark = spark.max(spark_value(full));
            }
            f.render_widget(spark, body);
        }
    }
}

/// Thick filled part, thin dim remainder: readable with or without color.
fn bar(ratio: f64, width: u16) -> Line<'static> {
    let filled = (ratio * f64::from(width)).round() as usize;
    let rest = usize::from(width).saturating_sub(filled);
    Line::from(vec![
        Span::styled("━".repeat(filled), Style::new().fg(level_color(ratio))),
        Span::styled("─".repeat(rest), Style::new().dark_gray()),
    ])
}

/// Sparklines take integers; keep two decimals so small values like load averages still show.
fn spark_value(v: f64) -> u64 {
    (v.max(0.0) * 100.0).round() as u64
}

fn level_color(ratio: f64) -> Color {
    match Level::of(ratio) {
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

    struct Fake(f64);

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
            self.0 += 1.0;
            out.push(LOAD, 42.0);
            // Like a real counter module: no rate until a second total exists.
            if self.0 > 1.0 {
                out.push(RATE, 1024.0);
            }
        }

        fn panels(&self) -> Vec<Panel> {
            vec![
                Panel::new("test", "Fake")
                    .widget(Widget::gauge("load", LOAD))
                    .widget(Widget::series("rate", RATE)),
            ]
        }
    }

    fn render(width: u16, height: u16) -> String {
        let mut host = Host::new(vec![Box::new(Fake(0.0))]);
        host.tick();
        let panels = host.panels();
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal
            .draw(|f| frame(f, &host, &panels, Duration::from_millis(500)))
            .unwrap();
        terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|c| c.symbol())
            .collect()
    }

    #[test]
    fn draws_panels_and_values() {
        let screen = render(80, 10);
        assert!(screen.contains(" Fake "));
        assert!(screen.contains("load"));
        assert!(screen.contains("42.0%"));
        assert!(screen.contains("━") && screen.contains("─"));
        // One tick gives a single counter total, so no rate yet.
        assert!(screen.contains("–"));
        assert!(screen.contains("sampling every 500ms"));
    }

    #[test]
    fn survives_tiny_terminals() {
        render(10, 3);
        render(1, 1);
    }
}
