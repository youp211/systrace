//! Desktop front end (egui).

use std::io;
use std::time::{Duration, Instant};

use crate::display::{self, Level, Panel, Widget};
use crate::host::Host;
use crate::metric::Unit;
use crate::pace::Pace;
use crate::{fmt, layout};
use eframe::egui::{
    self, Align, Color32, Label, Layout, Pos2, ProgressBar, Rect, Sense, Shape, Stroke, Ui, Vec2,
    pos2,
};

/// Narrowest column before the layout drops to fewer columns.
const MIN_COLUMN_WIDTH: f32 = 380.0;
const LABEL_WIDTH: f32 = 130.0;
const VALUE_WIDTH: f32 = 90.0;
const ROW_HEIGHT: f32 = 18.0;
/// Horizontal pixels per sample in a sparkline.
const SAMPLE_WIDTH: f32 = 2.0;

/// Opens a window that samples `host` every `interval` until it is closed.
pub fn run(host: Host, interval: Duration) -> io::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("systrace")
            .with_inner_size([1200.0, 800.0]),
        ..Default::default()
    };
    eframe::run_native(
        "systrace",
        options,
        Box::new(move |_cc| Ok(Box::new(App::new(host, interval)))),
    )
    .map_err(|e| io::Error::other(e.to_string()))
}

struct App {
    host: Host,
    /// Fixed for the session, since they only depend on probe results.
    panels: Vec<Panel>,
    pace: Pace,
}

impl App {
    fn new(mut host: Host, interval: Duration) -> Self {
        let panels = host.panels();
        host.tick();
        Self {
            host,
            panels,
            pace: Pace::new(interval, Instant::now()),
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let now = Instant::now();
        if self.pace.due(now) {
            self.host.tick();
            self.pace.advance(Instant::now());
        }
        // Repaint at the next sample rather than every frame; input still repaints immediately.
        ctx.request_repaint_after(self.pace.wait(Instant::now()));

        egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
            ui.weak(format!(
                "sampling every {}",
                fmt::interval(self.pace.interval())
            ));
        });
        egui::CentralPanel::default().show(ctx, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                let count = (ui.available_width() / MIN_COLUMN_WIDTH).floor().max(1.0) as usize;
                let columns = layout::columns(&self.panels, count, layout::rows);
                ui.columns(columns.len(), |uis| {
                    for (column, ui) in columns.iter().zip(uis.iter_mut()) {
                        for &i in column {
                            draw_panel(ui, &self.host, &self.panels[i]);
                        }
                    }
                });
            });
        });
    }
}

fn draw_panel(ui: &mut Ui, host: &Host, panel: &Panel) {
    egui::Frame::group(ui.style()).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.strong(&panel.title);
        for widget in &panel.widgets {
            draw_row(ui, host, widget);
        }
    });
    ui.add_space(6.0);
}

fn draw_row(ui: &mut Ui, host: &Host, widget: &Widget) {
    let store = host.store();
    let unit = store.desc(widget.metric()).map(|d| d.unit);
    let latest = store.latest(widget.metric());

    ui.horizontal(|ui| {
        ui.add_sized(
            [LABEL_WIDTH, ROW_HEIGHT],
            Label::new(widget.label()).truncate(),
        );
        let width = (ui.available_width() - VALUE_WIDTH).max(40.0);
        match widget {
            Widget::Gauge { .. } => {
                match latest.zip(unit).and_then(|(v, u)| display::ratio(v, u)) {
                    Some(ratio) => {
                        let bar = ProgressBar::new(ratio as f32)
                            .desired_width(width)
                            .desired_height(ROW_HEIGHT * 0.6)
                            .fill(level_color(widget.level(ratio)));
                        ui.add(bar);
                    }
                    // Only units with a fixed full scale get a bar; other gauges show just the value.
                    None => ui.add_space(width),
                }
            }
            Widget::Series { .. } => {
                let (rect, _) =
                    ui.allocate_exact_size(Vec2::new(width, ROW_HEIGHT), Sense::hover());
                let history: Vec<_> = store.history(widget.metric()).collect();
                let stroke = Stroke::new(1.5_f32, ui.visuals().selection.bg_fill);
                let painter = ui.painter_at(rect);
                for segment in sparkline(&history, unit, rect) {
                    painter.add(Shape::line(segment, stroke));
                }
            }
        }
        let text = fmt::reading(latest, unit);
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.monospace(text)
        });
    });
}

/// Polylines for a sparkline in `rect`, newest sample at the right edge, scaled
/// by `display::spark_heights`. Gaps split the line so missing data never looks
/// like a drop to zero.
fn sparkline(history: &[Option<f64>], unit: Option<Unit>, rect: Rect) -> Vec<Vec<Pos2>> {
    let fits = (rect.width() / SAMPLE_WIDTH) as usize + 1;
    let shown = &history[history.len().saturating_sub(fits)..];
    let heights = display::spark_heights(shown, unit);

    let mut segments = vec![Vec::new()];
    for (age, height) in heights.iter().rev().enumerate() {
        match height {
            Some(h) => {
                let x = rect.right() - age as f32 * SAMPLE_WIDTH;
                let y = rect.bottom() - *h as f32 * rect.height();
                if let Some(current) = segments.last_mut() {
                    current.push(pos2(x, y));
                }
            }
            None => segments.push(Vec::new()),
        }
    }
    // A single point can't draw a line; a lone sample between gaps stays invisible.
    segments.retain(|s| s.len() >= 2);
    segments
}

fn level_color(level: Level) -> Color32 {
    match level {
        Level::Critical => Color32::from_rgb(214, 69, 65),
        Level::High => Color32::from_rgb(230, 170, 40),
        Level::Normal => Color32::from_rgb(76, 175, 80),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect() -> Rect {
        Rect::from_min_size(pos2(0.0, 0.0), Vec2::new(10.0, 10.0))
    }

    #[test]
    fn splits_on_gaps_and_scales() {
        let history = [Some(0.0), Some(50.0), None, Some(100.0), Some(25.0)];
        let segments = sparkline(&history, Some(Unit::Percent), rect());
        // Newest first: [25, 100] at the right edge, then [50, 0] after the gap.
        assert_eq!(segments.len(), 2);
        assert_eq!(segments[0], [pos2(10.0, 7.5), pos2(8.0, 0.0)]);
        assert_eq!(segments[1], [pos2(4.0, 5.0), pos2(2.0, 10.0)]);
    }

    #[test]
    fn scales_to_data_without_full_scale() {
        let segments = sparkline(&[Some(2.0), Some(4.0)], Some(Unit::Bytes), rect());
        assert_eq!(segments, [vec![pos2(10.0, 0.0), pos2(8.0, 5.0)]]);
        assert!(sparkline(&[None, Some(1.0), None], None, rect()).is_empty());
        assert!(sparkline(&[], None, rect()).is_empty());
    }

    #[test]
    fn keeps_only_what_fits() {
        let history = vec![Some(1.0); 100];
        let segments = sparkline(&history, None, rect());
        assert_eq!(segments[0].len(), 6);
    }

    #[test]
    fn huge_values_stay_on_screen() {
        let segments = sparkline(&[Some(3e16), Some(1.0)], Some(Unit::BytesPerSec), rect());
        assert!(
            segments[0]
                .iter()
                .all(|p| p.y.is_finite() && (0.0..=10.0).contains(&p.y))
        );
    }
}
