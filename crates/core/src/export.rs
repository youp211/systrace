//! Output formats shared by every front end.

use std::io::{self, Write};

use serde::Serialize;

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

fn table<R: Record>(w: &mut impl Write, records: &[R]) -> io::Result<()> {
    let header: Vec<String> = R::table_header().iter().map(|h| h.to_uppercase()).collect();
    let rows: Vec<Vec<String>> = records.iter().map(Record::table_cells).collect();

    let mut widths: Vec<usize> = header.iter().map(|h| h.chars().count()).collect();
    for row in &rows {
        for (width, cell) in widths.iter_mut().zip(row) {
            *width = (*width).max(cell.chars().count());
        }
    }

    for row in std::iter::once(&header).chain(&rows) {
        let line: Vec<String> = row
            .iter()
            .zip(&widths)
            .map(|(c, w)| format!("{c:<w$}"))
            .collect();
        writeln!(w, "{}", line.join("  ").trim_end())?;
    }
    Ok(())
}

fn csv_escape(cell: &str) -> String {
    if cell.contains([',', '"', '\n']) {
        format!("\"{}\"", cell.replace('"', "\"\""))
    } else {
        cell.to_string()
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
}
