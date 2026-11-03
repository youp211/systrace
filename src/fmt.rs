//! Human-readable value formatting.

use std::time::Duration;

use crate::metric::Unit;

/// Sampling interval: `1s`, `500ms`.
pub fn interval(d: Duration) -> String {
    if d.subsec_millis() == 0 {
        format!("{}s", d.as_secs())
    } else {
        format!("{}ms", d.as_millis())
    }
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

fn scaled(mut v: f64, step: f64, units: &[&str]) -> String {
    let mut i = 0;
    while v.abs() >= step && i < units.len() - 1 {
        v /= step;
        i += 1;
    }
    if i == 0 {
        format!("{v:.0} {}", units[0])
    } else {
        format!("{v:.2} {}", units[i])
    }
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
}
