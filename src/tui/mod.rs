//! Interactive terminal front end.

mod draw;

use std::io;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use ratatui::DefaultTerminal;
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};

use crate::host::Host;
use crate::pace::Pace;

/// Longest wait between checks for a termination signal.
const SIGNAL_CHECK: Duration = Duration::from_millis(250);

/// Samples `host` every `interval` and redraws until the user quits or the
/// process is told to stop; the terminal is restored either way.
pub fn run(mut host: Host, interval: Duration) -> io::Result<()> {
    let stop = stop_signals()?;
    let mut terminal = match ratatui::try_init() {
        Ok(terminal) => terminal,
        Err(e) => {
            // Initialization can fail after raw mode is already on.
            ratatui::restore();
            return Err(e);
        }
    };
    let result = event_loop(&mut terminal, &mut host, interval, &stop);
    ratatui::restore();
    result
}

/// A flag raised by SIGTERM, SIGHUP or SIGINT, so the loop can exit and restore
/// the terminal instead of leaving it in raw mode.
#[cfg(unix)]
fn stop_signals() -> io::Result<Arc<AtomicBool>> {
    use signal_hook::consts::{SIGHUP, SIGINT, SIGTERM};
    let stop = Arc::new(AtomicBool::new(false));
    for signal in [SIGTERM, SIGHUP, SIGINT] {
        signal_hook::flag::register(signal, Arc::clone(&stop))?;
    }
    Ok(stop)
}

#[cfg(not(unix))]
fn stop_signals() -> io::Result<Arc<AtomicBool>> {
    Ok(Arc::new(AtomicBool::new(false)))
}

fn event_loop(
    terminal: &mut DefaultTerminal,
    host: &mut Host,
    interval: Duration,
    stop: &AtomicBool,
) -> io::Result<()> {
    // Panels only depend on probe results, which are fixed for the session.
    let panels = host.panels();
    host.tick();
    let mut pace = Pace::new(interval, Instant::now());
    let mut redraw = true;

    loop {
        if stop.load(Ordering::Relaxed) {
            return Ok(());
        }
        if redraw {
            terminal.draw(|f| draw::frame(f, host, &panels, interval))?;
            redraw = false;
        }
        let now = Instant::now();
        if pace.due(now) {
            host.tick();
            pace.advance(Instant::now());
            redraw = true;
            continue;
        }
        // Wait for input until the next tick; a key or resize only redraws.
        if event::poll(pace.wait(now).min(SIGNAL_CHECK))? {
            let event = event::read()?;
            if quits(&event) {
                return Ok(());
            }
            redraw = true;
        }
    }
}

/// `q`, Esc or Ctrl-C, on key press only.
fn quits(event: &Event) -> bool {
    let Event::Key(key) = event else {
        return false;
    };
    key.kind == KeyEventKind::Press
        && (matches!(key.code, KeyCode::Char('q') | KeyCode::Esc)
            || (key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::crossterm::event::{KeyEvent, KeyEventState};

    fn key(code: KeyCode, modifiers: KeyModifiers, kind: KeyEventKind) -> Event {
        Event::Key(KeyEvent {
            code,
            modifiers,
            kind,
            state: KeyEventState::NONE,
        })
    }

    #[test]
    fn quit_keys() {
        let press = KeyEventKind::Press;
        assert!(quits(&key(KeyCode::Char('q'), KeyModifiers::NONE, press)));
        assert!(quits(&key(KeyCode::Esc, KeyModifiers::NONE, press)));
        assert!(quits(&key(
            KeyCode::Char('c'),
            KeyModifiers::CONTROL,
            press
        )));
        assert!(!quits(&key(KeyCode::Char('c'), KeyModifiers::NONE, press)));
        assert!(!quits(&key(
            KeyCode::Char('q'),
            KeyModifiers::NONE,
            KeyEventKind::Release
        )));
        assert!(!quits(&Event::Resize(80, 24)));
    }
}
