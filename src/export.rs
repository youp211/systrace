//! Output formats shared by every front end.

use std::io::{self, Write};

use serde::Serialize;
use unicode_width::UnicodeWidthStr;

use crate::util::text::printable;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    Table,
    Json,
    Ndjson,
    Csv,
}

/// A row that can be written in every format.
pub trait Record: Serialize {
    fn header() -> &'static [&'static str];

    /// Table header; override when table cells differ from raw cells.
    fn table_header() -> &'static [&'static str] {
        Self::header()
    }

    /// Raw cells, used for CSV.
    fn cells(&self) -> Vec<String>;

    /// Human-readable cells, used for tables.
    fn table_cells(&self) -> Vec<String> {
        self.cells()
    }
}

pub fn write<R: Record>(w: &mut impl Write, records: &[R], format: Format) -> io::Result<()> {
    debug_assert!(
        records.iter().all(|r| r.cells().len() == R::header().len()
            && r.table_cells().len() == R::table_header().len()),
        "a Record's cells must match its header"
    );
    match format {
        Format::Table => table(w, records),
        Format::Json => {
            serde_json::to_writer_pretty(&mut *w, records)?;
            writeln!(w)
        }
        Format::Ndjson => records.iter().try_for_each(|r| {
            serde_json::to_writer(&mut *w, r)?;
            writeln!(w)
        }),
        Format::Csv => {
            writeln!(w, "{}", R::header().join(","))?;
            records.iter().try_for_each(|r| {
                let cells: Vec<_> = r.cells().iter().map(|c| csv_escape(c)).collect();
                writeln!(w, "{}", cells.join(","))
            })
        }
    }
}

/// Columns padded by display width; control characters can't reach the terminal.
fn table<R: Record>(w: &mut impl Write, records: &[R]) -> io::Result<()> {
    let header: Vec<String> = R::table_header().iter().map(|h| h.to_uppercase()).collect();
    let rows: Vec<Vec<String>> = records
        .iter()
        .map(|r| {
            r.table_cells()
                .iter()
                .map(|c| printable(c).into_owned())
                .collect()
        })
        .collect();

    let mut widths: Vec<usize> = header.iter().map(|h| h.width()).collect();
    for row in &rows {
        for (width, cell) in widths.iter_mut().zip(row) {
            *width = (*width).max(cell.width());
        }
    }

    for row in std::iter::once(&header).chain(&rows) {
        let mut line = String::new();
        for (i, (cell, width)) in row.iter().zip(&widths).enumerate() {
            if i > 0 {
                line.push_str("  ");
            }
            line.push_str(cell);
            line.extend(std::iter::repeat_n(' ', width.saturating_sub(cell.width())));
        }
        writeln!(w, "{}", line.trim_end())?;
    }
    Ok(())
}

/// RFC 4180 quoting, plus a leading `'` on text a spreadsheet would run as a
/// formula (`=`, `+`, `-`, `@`); plain numbers like `-1.5` stay as they are.
fn csv_escape(cell: &str) -> String {
    let formula =
        cell.starts_with(['=', '+', '-', '@', '\t', '\r']) && cell.parse::<f64>().is_err();
    let cell = if formula {
        format!("'{cell}")
    } else {
        cell.to_string()
    };
    if cell.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", cell.replace('"', "\"\""))
    } else {
        cell
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Serialize)]
    struct Row {
        name: &'static str,
        value: f64,
    }

    impl Record for Row {
        fn header() -> &'static [&'static str] {
            &["name", "value"]
        }

        fn cells(&self) -> Vec<String> {
            vec![self.name.to_string(), self.value.to_string()]
        }
    }

    fn render(format: Format) -> String {
        let rows = [
            Row {
                name: "a,b",
                value: 1.5,
            },
            Row {
                name: "c",
                value: 2.0,
            },
        ];
        let mut out = Vec::new();
        write(&mut out, &rows, format).unwrap();
        String::from_utf8(out).unwrap()
    }

    #[test]
    fn writes_formats() {
        assert_eq!(render(Format::Csv), "name,value\n\"a,b\",1.5\nc,2\n");
        assert_eq!(
            render(Format::Ndjson),
            "{\"name\":\"a,b\",\"value\":1.5}\n{\"name\":\"c\",\"value\":2.0}\n"
        );
        assert_eq!(render(Format::Table), "NAME  VALUE\na,b   1.5\nc     2\n");
    }

    #[test]
    fn csv_quotes_line_breaks_and_defuses_formulas() {
        assert_eq!(csv_escape("a\rb"), "\"a\rb\"");
        assert_eq!(
            csv_escape("=HYPERLINK(\"x\")"),
            "\"'=HYPERLINK(\"\"x\"\")\""
        );
        assert_eq!(csv_escape("@SUM(A1)"), "'@SUM(A1)");
        assert_eq!(csv_escape("-1.5"), "-1.5");
        assert_eq!(csv_escape("plain"), "plain");
    }

    #[test]
    fn table_strips_control_characters_and_aligns_wide_text() {
        let rows = [
            Row {
                name: "\x1b[2J外部",
                value: 1.0,
            },
            Row {
                name: "ab",
                value: 2.0,
            },
        ];
        let mut out = Vec::new();
        write(&mut out, &rows, Format::Table).unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(
            !text.chars().any(|c| c.is_control() && c != '\n'),
            "{text:?}"
        );
        // Every value starts in the same display column.
        let columns: Vec<usize> = text
            .lines()
            .skip(1)
            .map(|l| l.rfind(' ').map_or(0, |i| l[..=i].width()))
            .collect();
        assert!(columns.windows(2).all(|w| w[0] == w[1]), "{text}");
    }
}
