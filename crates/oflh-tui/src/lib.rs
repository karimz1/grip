//! A single-owner UI with bounded background work and event-driven rendering.
#![forbid(unsafe_code)]
mod app;
mod view;
mod worker;
use app::{App, Effect, Screen};
use crossterm::event::{self, Event as TerminalEvent, KeyCode, KeyEventKind, KeyModifiers};
use oflh_core::*;
use oflh_platform::Backend;
use std::{
    sync::mpsc::{self, RecvTimeoutError},
    time::{Duration, Instant},
};
use worker::{Work, Worker};
enum Event {
    Input(std::io::Result<TerminalEvent>),
    Scan(u64, Result<Snapshot>),
    Metrics(u64, Result<Vec<(Identity, Metrics)>>),
    Killed(usize, Vec<String>),
}
const REFRESH: Duration = Duration::from_secs(5);
const PULSE: Duration = Duration::from_millis(120);
struct TerminalGuard;
impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = crossterm::execute!(std::io::stdout(), event::DisableBracketedPaste);
        ratatui::restore();
    }
}
pub fn run(target: Target, version: String, backend: Box<dyn Backend>) -> std::io::Result<()> {
    let mut terminal = ratatui::try_init()?;
    let _guard = TerminalGuard;
    crossterm::execute!(std::io::stdout(), event::EnableBracketedPaste)?;
    let mut app = App::new(target, version);
    app.scanning = true;
    terminal.draw(|frame| view::draw(frame, &mut app))?;
    let (sender, receiver) = mpsc::sync_channel(128);
    let mut worker = Worker::new(backend, sender.clone())?;
    std::thread::Builder::new()
        .name("oflh-input".into())
        .spawn(move || {
            loop {
                let event = event::read();
                let failed = event.is_err();
                if sender.send(Event::Input(event)).is_err() || failed {
                    break;
                }
            }
        })?;
    let mut generation = 1;
    worker.request(generation, Work::Scan(app.target.clone()));
    let mut pulse = Instant::now() + PULSE;
    let mut refresh = Instant::now() + REFRESH;
    let mut sample: Option<Instant> = None;
    let mut sampling = false;
    loop {
        let now = Instant::now();
        let mut deadline = now + Duration::from_secs(86400);
        if app.scanning || app.stopping {
            deadline = deadline.min(pulse)
        }
        if app.auto {
            deadline = deadline.min(refresh)
        }
        if let Some(at) = sample {
            deadline = deadline.min(at)
        }
        let mut dirty = false;
        let mut effect = Effect::None;
        match receiver.recv_timeout(deadline.saturating_duration_since(now)) {
            Ok(Event::Input(Ok(TerminalEvent::Key(key)))) if key.kind != KeyEventKind::Release => {
                let auto = app.auto;
                if !app.editing
                    && app.screen == Screen::Main
                    && app.tree.is_none()
                    && key.code == KeyCode::Char('a')
                    && key.modifiers.contains(KeyModifiers::CONTROL)
                {
                    app.select_all()
                } else {
                    effect = app.key(key)
                }
                if auto != app.auto {
                    refresh = Instant::now() + REFRESH
                }
                dirty = true;
            }
            Ok(Event::Input(Ok(TerminalEvent::Paste(s)))) => {
                app.paste(&s);
                dirty = true
            }
            Ok(Event::Input(Ok(TerminalEvent::Resize(..)))) => dirty = true,
            Ok(Event::Input(Err(e))) => return Err(e),
            Ok(Event::Scan(g, result)) if g == generation => {
                app.scanning = false;
                sampling = false;
                match result {
                    Ok(snapshot) => {
                        app.replace(snapshot);
                        sample = Some(Instant::now() + Duration::from_secs(1))
                    }
                    Err(Error::Cancelled) => {}
                    Err(e) => {
                        app.status = format!("Scan failed: {e}");
                        app.error = true
                    }
                }
                dirty = true
            }
            Ok(Event::Metrics(g, result)) if g == generation => {
                sampling = false;
                if let Ok(metrics) = result {
                    app.metrics(metrics)
                }
                dirty = true
            }
            Ok(Event::Killed(sent, errors)) => {
                app.stopping = false;
                app.error = !errors.is_empty();
                app.status = format!(
                    "{sent} termination requests sent{}",
                    if errors.is_empty() {
                        String::new()
                    } else {
                        format!(" · {}", errors.join("; "))
                    }
                );
                if sent > 0 {
                    app.tree = None;
                    app.selected.clear()
                }
                effect = Effect::Scan;
                dirty = true
            }
            Err(RecvTimeoutError::Disconnected) => return Ok(()),
            _ => {}
        }
        match effect {
            Effect::Quit => return Ok(()),
            Effect::Scan => {
                generation += 1;
                sample = None;
                sampling = false;
                app.scanning = true;
                worker.request(generation, Work::Scan(app.target.clone()));
                pulse = Instant::now() + PULSE;
                dirty = true
            }
            Effect::Kill(ids, force) => {
                generation += 1;
                sample = None;
                app.scanning = false;
                sampling = false;
                worker.request(generation, Work::Kill(ids, force));
                pulse = Instant::now() + PULSE;
                dirty = true
            }
            Effect::Link(url) => {
                if let Err(e) = open_link(url) {
                    app.status = format!("Open browser: {e}");
                    app.error = true
                }
                dirty = true
            }
            Effect::None => {}
        }
        let now = Instant::now();
        if (app.scanning || app.stopping) && now >= pulse {
            app.pulse = (app.pulse + 1) % 4;
            pulse = now + PULSE;
            dirty = true
        }
        if app.auto && now >= refresh {
            refresh = now + REFRESH;
            if !app.scanning && !app.stopping && !sampling {
                generation += 1;
                sample = None;
                app.scanning = true;
                worker.request(generation, Work::Scan(app.target.clone()));
                pulse = now + PULSE;
                dirty = true
            }
        }
        if sample.is_some_and(|at| now >= at) {
            sample = None;
            if !app.scanning && !app.stopping {
                sampling = true;
                worker.request(
                    generation,
                    Work::Sample(app.snapshot.processes.iter().map(|p| p.identity).collect()),
                )
            }
        }
        if dirty {
            terminal.draw(|frame| view::draw(frame, &mut app))?;
        }
    }
}
fn open_link(url: &'static str) -> std::io::Result<()> {
    use std::process::{Command, Stdio};
    #[cfg(target_os = "linux")]
    let mut command = {
        let mut c = Command::new("xdg-open");
        c.arg(url);
        c
    };
    #[cfg(target_os = "macos")]
    let mut command = {
        let mut c = Command::new("open");
        c.arg(url);
        c
    };
    #[cfg(windows)]
    let mut command = {
        let mut c = Command::new("rundll32.exe");
        c.args(["url.dll,FileProtocolHandler", url]);
        c
    };
    #[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
    return Err(std::io::Error::other("unsupported platform"));
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}
#[cfg(test)]
mod tests;
