//! Human-readable value formatting.

use std::time::Duration;

use crate::metric::Unit;

/// Sampling interval: `1s`, `500ms`.
pub fn interval(d: Duration) -> String {
    if d.subsec_nanos() == 0 {
        format!("{}s", d.as_secs())
    } else {
        format!("{}ms", d.as_millis())
    }
}

/// A metric's latest value for display, or a dash when it has none this tick.
pub fn reading(v: Option<f64>, unit: Option<Unit>) -> String {
    v.zip(unit).map_or_else(|| "–".into(), |(v, u)| value(v, u))
}

pub fn value(v: f64, unit: Unit) -> String {
    match unit {
        Unit::Percent => format!("{v:.1}%"),
        Unit::Bytes => bytes(v),
        Unit::BytesPerSec => format!("{}/s", bytes(v)),
        Unit::Hertz => scaled(v, 1000.0, &["Hz", "kHz", "MHz", "GHz"]),
        Unit::Celsius => format!("{v:.1} °C"),
        Unit::Count => format!("{v:.0}"),
        Unit::PerSec => format!("{v:.1}/s"),
        Unit::Number => format!("{v:.2}"),
    }
}

fn bytes(v: f64) -> String {
    scaled(v, 1024.0, &["B", "KiB", "MiB", "GiB", "TiB"])
}

/// Picks the unit after rounding, so 1048575 B is `1.00 MiB`, not `1024.00 KiB`.
fn scaled(v: f64, step: f64, units: &[&str]) -> String {
    let mut x = v;
    for (i, unit) in units.iter().enumerate() {
        let decimals = if i == 0 { 0 } else { 2 };
        let scale = 10f64.powi(decimals);
        let shown = (x * scale).round() / scale;
        if shown.abs() < step || i == units.len() - 1 {
            let decimals = decimals as usize;
            return format!("{x:.decimals$} {unit}");
        }
        x /= step;
    }
    format!("{v}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_units() {
        assert_eq!(value(23.456, Unit::Percent), "23.5%");
        assert_eq!(value(512.0, Unit::Bytes), "512 B");
        assert_eq!(value(1536.0 * 1024.0, Unit::Bytes), "1.50 MiB");
        assert_eq!(value(2048.0, Unit::BytesPerSec), "2.00 KiB/s");
        assert_eq!(value(3_200_000_000.0, Unit::Hertz), "3.20 GHz");
        assert_eq!(value(41.25, Unit::Celsius), "41.2 °C");
        assert_eq!(value(7.0, Unit::Count), "7");
        assert_eq!(value(12.34, Unit::PerSec), "12.3/s");
        assert_eq!(value(1.234, Unit::Number), "1.23");
        assert_eq!(interval(Duration::from_secs(2)), "2s");
        assert_eq!(interval(Duration::from_millis(250)), "250ms");
    }

    #[test]
    fn rounds_before_choosing_unit() {
        assert_eq!(value(1_048_575.0, Unit::Bytes), "1.00 MiB");
        assert_eq!(value(1023.6, Unit::Bytes), "1.00 KiB");
        assert_eq!(value(1023.4, Unit::Bytes), "1023 B");
        assert_eq!(value(999_996_000.0, Unit::Hertz), "1.00 GHz");
        assert_eq!(value(1024.0_f64.powi(6), Unit::Bytes), "1048576.00 TiB");
    }

    #[test]
    fn readings_show_a_dash_when_missing() {
        assert_eq!(reading(Some(50.0), Some(Unit::Percent)), "50.0%");
        assert_eq!(reading(None, Some(Unit::Percent)), "–");
        assert_eq!(reading(Some(1.0), None), "–");
    }
}
