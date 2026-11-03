//! Interactive terminal front end.

mod draw;

use std::io;
use std::time::{Duration, Instant};

use crate::host::Host;
use ratatui::DefaultTerminal;
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

/// Samples `host` every `interval` and redraws until the user quits.
pub fn run(mut host: Host, interval: Duration) -> io::Result<()> {
    let mut terminal = ratatui::try_init()?;
    let result = event_loop(&mut terminal, &mut host, interval);
    ratatui::restore();
    result
}

fn event_loop(
    terminal: &mut DefaultTerminal,
    host: &mut Host,
    interval: Duration,
) -> io::Result<()> {
    // Panels only depend on probe results, which are fixed for the session.
    let panels = host.panels();
    host.tick();
    let mut next = Instant::now() + interval;

    loop {
        terminal.draw(|f| draw::frame(f, host, &panels, interval))?;

        // Wait for input until the next tick is due; a key or resize just redraws.
        if event::poll(next.saturating_duration_since(Instant::now()))? {
            match event::read()? {
                Event::Key(key) if key.kind == KeyEventKind::Press && is_quit(key) => return Ok(()),
                _ => continue,
            }
        }

        host.tick();
        // Keep a steady cadence, but don't burst to catch up after a stall.
        next = (next + interval).max(Instant::now());
    }
}

fn is_quit(key: KeyEvent) -> bool {
    matches!(key.code, KeyCode::Char('q') | KeyCode::Esc)
        || (key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL))
}
