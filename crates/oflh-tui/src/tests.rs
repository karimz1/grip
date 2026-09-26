use super::*;
use app::*;
use crossterm::event::{KeyCode as K, KeyEvent, KeyModifiers};
fn app() -> App {
    let mut app = App::new(Target::new(std::env::temp_dir()).unwrap(), "test".into());
    app.replace(Snapshot {
        processes: vec![Process {
            identity: Identity {
                pid: 424242,
                started: 10,
                started_sub: 0,
            },
            name: "dotnet".into(),
            user: "alice".into(),
            usages: vec![
                Usage {
                    path: "/build/FileLockExampleCli.dll".into(),
                    relation: Relation::Locked,
                    access: Access::ReadWrite,
                    lock: Some(LockEvidence::Kernel("POSIX WRITE".into())),
                    ..Usage::default()
                },
                Usage {
                    path: "/build/FileLockExampleCli.deps.json".into(),
                    ..Usage::default()
                },
                Usage {
                    path: "/build/other.dll".into(),
                    ..Usage::default()
                },
            ],
            ancestors: vec![Ancestor {
                identity: Identity {
                    pid: 424241,
                    started: 9,
                    started_sub: 0,
                },
                name: "parent".into(),
            }],
            ..Process::default()
        }],
        warnings: vec![],
    });
    app
}
fn key(a: &mut App, k: K) -> Effect {
    a.key(KeyEvent::new(k, KeyModifiers::NONE))
}
#[test]
fn search_and_details() {
    let mut a = app();
    key(&mut a, K::Char('/'));
    a.paste("424242 FLEC*.json");
    key(&mut a, K::Enter);
    assert_eq!(a.rows.len(), 1);
    assert_eq!(a.rows[0].usages.len(), 1);
    key(&mut a, K::Enter);
    assert_eq!(a.screen, Screen::Details);
    assert_eq!(a.detail_query, "flec*.json");
    assert_eq!(a.usage_rows.len(), 1);
    key(&mut a, K::Esc);
    assert_eq!(a.usage_rows.len(), 3);
    key(&mut a, K::Char('/'));
    a.paste("kxqr");
    assert!(!a.stopping);
    key(&mut a, K::Esc);
    assert!(a.detail_query.is_empty());
    key(&mut a, K::Esc);
    assert_eq!(a.screen, Screen::Main);
    assert_eq!(a.query, "424242 FLEC*.json");
}
#[test]
fn safe_confirmation_and_hidden_selection() {
    let mut a = app();
    key(&mut a, K::Char(' '));
    key(&mut a, K::Char('/'));
    a.paste("absent");
    key(&mut a, K::Enter);
    assert!(a.rows.is_empty());
    key(&mut a, K::Char('x'));
    assert_eq!(a.pending.len(), 1);
    assert!(!a.confirm);
    assert!(matches!(key(&mut a, K::Enter), Effect::None));
    assert!(!a.stopping);
    key(&mut a, K::Char('x'));
    key(&mut a, K::Tab);
    a.width = 10;
    assert!(matches!(key(&mut a, K::Enter), Effect::None));
    a.width = 80;
    assert!(matches!(key(&mut a,K::Enter),Effect::Kill(ids,true) if ids.len()==1));
}
#[test]
fn ancestry_captures_identity() {
    let mut a = app();
    key(&mut a, K::Tab);
    key(&mut a, K::Up);
    let mut snapshot = a.snapshot.clone();
    snapshot.processes[0].ancestors[0].identity.started = 99;
    a.replace(snapshot);
    key(&mut a, K::Char('k'));
    assert_eq!(a.pending[0].identity.started, 9);
    assert!(a.parent_action);
}
#[test]
fn refresh_rejects_reused_detail() {
    let mut a = app();
    key(&mut a, K::Enter);
    let mut snap = a.snapshot.clone();
    snap.processes[0].identity.started = 99;
    a.replace(snap);
    assert!(a.detail().is_none());
    assert!(a.usage_rows.is_empty());
}
#[test]
fn rendering_all_sizes_and_screens() {
    use ratatui::{Terminal, backend::TestBackend};
    let mut a = app();
    for (w, h) in [(1, 1), (28, 18), (48, 20), (80, 24), (120, 40), (160, 50)] {
        for screen in [Screen::Main, Screen::Details, Screen::Help, Screen::Confirm] {
            a.screen = Screen::Main;
            key(&mut a, K::Enter);
            a.screen = screen;
            let mut terminal = Terminal::new(TestBackend::new(w, h)).unwrap();
            terminal.draw(|f| view::draw(f, &mut a)).unwrap();
            assert_eq!(terminal.backend().buffer().area.width, w);
        }
    }
}
#[test]
fn terminal_text_is_sanitized() {
    let mut a = app();
    a.snapshot.processes[0].name = "bad\x1b]52;payload\x07".into();
    let mut t = ratatui::Terminal::new(ratatui::backend::TestBackend::new(160, 40)).unwrap();
    t.draw(|f| view::draw(f, &mut a)).unwrap();
    let text = t
        .backend()
        .buffer()
        .content
        .iter()
        .map(|c| c.symbol())
        .collect::<String>();
    assert!(!text.contains('\x1b'));
    assert!(!text.contains('\x07'));
}

#[test]
fn golden_screens() {
    let mut a = app();
    a.target.path = "/build".into();
    a.snapshot.processes[0].cpu = Some(2.4);
    a.snapshot.processes[0].memory = Some(31 * 1024 * 1024);
    a.snapshot.processes[0].executable = "/usr/bin/dotnet".into();
    a.snapshot.processes[0].cwd = "/build".into();
    for (name, screen, locked, w, h) in [
        ("processes", Screen::Main, false, 160, 40),
        ("locks", Screen::Main, true, 120, 30),
        ("details", Screen::Details, false, 160, 40),
        ("compact", Screen::Details, false, 48, 20),
    ] {
        a.screen = Screen::Main;
        a.locked = locked;
        a.refilter();
        if screen == Screen::Details {
            key(&mut a, K::Enter);
        }
        a.screen = screen;
        let mut terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(w, h)).unwrap();
        terminal.draw(|f| view::draw(f, &mut a)).unwrap();
        let buffer = terminal.backend().buffer();
        let mut text = String::new();
        for row in buffer.content.chunks(w as usize) {
            let line = row.iter().map(|c| c.symbol()).collect::<String>();
            text.push_str(line.trim_end());
            text.push('\n');
        }
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join(format!("tests/snapshots/{name}.txt"));
        if std::env::var_os("OFLH_UPDATE_SNAPSHOTS").is_some() {
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, &text).unwrap();
        }
        assert_eq!(
            std::fs::read_to_string(&path).expect("golden fixture exists"),
            text,
            "{name}"
        );
        export_visual(name, buffer);
    }
}
#[test]
fn focused_tree_survives_empty_refresh() {
    let mut a = app();
    key(&mut a, K::Tab);
    key(&mut a, K::Up);
    a.replace(Snapshot::default());
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(120, 30)).unwrap();
    terminal.draw(|f| view::draw(f, &mut a)).unwrap();
    let text = terminal
        .backend()
        .buffer()
        .content
        .iter()
        .map(|c| c.symbol())
        .collect::<String>();
    assert!(text.contains("parent (424241)"));
    key(&mut a, K::Char('k'));
    assert_eq!(a.pending[0].identity.pid, 424241);
}

/// Export rendered cell colors for optional visual review without changing fixtures.
fn export_visual(name: &str, buffer: &ratatui::buffer::Buffer) {
    use std::fmt::Write;
    let width = buffer.area.width;
    let height = buffer.area.height;
    if let Some(dir) = std::env::var_os("OFLH_VISUAL_DIR") {
        let mut svg = format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{}\" height=\"{}\"><rect width=\"100%\" height=\"100%\" fill=\"#202028\"/>",
            width * 9,
            height * 18
        );
        for row in 0..height {
            for column in 0..width {
                let cell = &buffer[(column, row)];
                let color = |c: ratatui::style::Color, default: &str| match c {
                    ratatui::style::Color::Rgb(r, g, b) => format!("#{r:02x}{g:02x}{b:02x}"),
                    ratatui::style::Color::DarkGray => "#74748a".into(),
                    ratatui::style::Color::White => "#ffffff".into(),
                    _ => default.into(),
                };
                let bg = color(cell.bg, "#202028");
                let fg = color(cell.fg, "#b8b8cc");
                if bg != "#202028" {
                    write!(
                        svg,
                        "<rect x=\"{}\" y=\"{}\" width=\"9\" height=\"18\" fill=\"{bg}\"/>",
                        column * 9,
                        row * 18
                    )
                    .unwrap();
                }
                let symbol = cell
                    .symbol()
                    .replace('&', "&amp;")
                    .replace('<', "&lt;")
                    .replace('>', "&gt;");
                if symbol != " " {
                    write!(svg,"<text x=\"{}\" y=\"{}\" fill=\"{fg}\" font-family=\"DejaVu Sans Mono\" font-size=\"14\">{symbol}</text>",column * 9,row * 18+14).unwrap();
                }
            }
        }
        svg.push_str("</svg>");
        let dir = std::path::Path::new(&dir);
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(dir.join(format!("{name}.svg")), svg).unwrap();
    }
}

fn label_background(buffer: &ratatui::buffer::Buffer, label: &str) -> ratatui::style::Color {
    let cells = buffer
        .content
        .windows(label.chars().count())
        .rev()
        .find(|cells| cells.iter().map(|cell| cell.symbol()).collect::<String>() == label)
        .expect("button label is visible");
    assert!(cells.iter().all(|cell| cell.bg == cells[0].bg));
    cells[0].bg
}

#[test]
fn confirmation_focus_has_distinct_backgrounds() {
    use ratatui::{Terminal, backend::TestBackend, style::Color};
    let mut application = app();
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
    terminal
        .draw(|frame| view::draw(frame, &mut application))
        .unwrap();
    key(&mut application, K::Char('k'));
    assert!(!application.confirm);
    terminal
        .draw(|frame| view::draw(frame, &mut application))
        .unwrap();
    let cancel_background = label_background(terminal.backend().buffer(), "Cancel");
    assert_ne!(cancel_background, Color::Reset);
    assert_eq!(
        label_background(terminal.backend().buffer(), "Terminate"),
        Color::Reset
    );
    export_visual("confirm-cancel", terminal.backend().buffer());
    key(&mut application, K::Tab);
    terminal
        .draw(|frame| view::draw(frame, &mut application))
        .unwrap();
    let terminate_background = label_background(terminal.backend().buffer(), "Terminate");
    assert_ne!(terminate_background, Color::Reset);
    assert_ne!(terminate_background, cancel_background);
    assert_eq!(
        label_background(terminal.backend().buffer(), "Cancel"),
        Color::Reset
    );
    export_visual("confirm-terminate", terminal.backend().buffer());
}

#[test]
fn help_links_precede_warnings_and_open_with_shortcuts() {
    let mut application = app();
    application.snapshot.warnings = vec!["Permission restricted".into(); 30];
    key(&mut application, K::Char('?'));
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(80, 24)).unwrap();
    terminal
        .draw(|frame| view::draw(frame, &mut application))
        .unwrap();
    let text = terminal
        .backend()
        .buffer()
        .content
        .iter()
        .map(|cell| cell.symbol())
        .collect::<String>();
    assert!(text.contains("https://github.com/karimz1/open-file-lock-handle"));
    assert!(text.contains("https://buymeacoffee.com/karimz1"));
    assert!(matches!(
        key(&mut application, K::Char('R')),
        Effect::Link("https://github.com/karimz1/open-file-lock-handle")
    ));
    assert!(matches!(
        key(&mut application, K::Char('D')),
        Effect::Link("https://buymeacoffee.com/karimz1")
    ));
    export_visual("help-links", terminal.backend().buffer());
}

fn ports_app() -> App {
    let mut application = app();
    application.target.path = "/build".into();
    let mut snapshot = application.snapshot.clone();
    snapshot.processes[0].executable = "/usr/bin/dotnet".into();
    snapshot.processes[0].cwd = "/build".into();
    snapshot.processes[0].ports = vec![
        Port {
            protocol: Protocol::Tcp,
            address: "127.0.0.1".parse().unwrap(),
            number: 3000,
        },
        Port {
            protocol: Protocol::Tcp,
            address: "::1".parse().unwrap(),
            number: 3000,
        },
        Port {
            protocol: Protocol::Udp,
            address: "127.0.0.1".parse().unwrap(),
            number: 5300,
        },
    ];
    let mut unrelated = snapshot.processes[0].clone();
    unrelated.identity.pid += 1;
    unrelated.name = "other-project".into();
    unrelated.usages.clear();
    unrelated.ports.truncate(1);
    snapshot.processes.push(unrelated);
    snapshot.processes.push(Process {
        name: "owner unavailable".into(),
        ports: vec![Port {
            protocol: Protocol::Tcp,
            address: "0.0.0.0".parse().unwrap(),
            number: 9000,
        }],
        ..Process::default()
    });
    application.replace(snapshot);
    application
}

#[test]
fn ports_scope_search_details_and_unknown_owner_actions() {
    let mut application = ports_app();
    assert!(matches!(key(&mut application, K::Char('3')), Effect::Scan));
    assert_eq!(application.rows.len(), 5);
    key(&mut application, K::Char('s'));
    assert_eq!(application.rows.len(), 3);
    key(&mut application, K::Char('/'));
    application.paste("3000 tcp");
    key(&mut application, K::Enter);
    assert_eq!(application.rows.len(), 2);
    key(&mut application, K::Char('K'));
    assert_eq!(
        application.pending.len(),
        1,
        "multiple bindings deduplicate action targets"
    );
    assert!(!application.confirm);
    key(&mut application, K::Esc);
    key(&mut application, K::Enter);
    assert!(application.detail_ports);
    assert_eq!(application.usage_rows.len(), 2);
    key(&mut application, K::Char('f'));
    assert!(!application.detail_ports);
    assert_eq!(application.usage_rows.len(), 3);
    key(&mut application, K::Esc);
    key(&mut application, K::Char('1'));
    assert_eq!(application.rows.len(), 1);
    assert!(
        application.query.is_empty(),
        "port terms do not pollute file search"
    );
    key(&mut application, K::Char('3'));
    assert_eq!(application.port_query, "3000 tcp");
    key(&mut application, K::Esc);
    key(&mut application, K::Char('s'));
    key(&mut application, K::End);
    key(&mut application, K::Char('k'));
    assert!(application.pending.is_empty());
    assert_eq!(application.screen, Screen::Main);
    assert!(application.error);
}

#[test]
fn port_selection_keeps_endpoint_and_birth_identity_across_refresh() {
    let mut application = ports_app();
    key(&mut application, K::Char('3'));
    key(&mut application, K::Char('s'));
    key(&mut application, K::Down);
    let mut snapshot = application.snapshot.clone();
    snapshot.processes[0].ports.reverse();
    application.replace(snapshot);
    let row = &application.rows[application.cursor];
    assert!(
        application.snapshot.processes[row.process].ports[row.port.unwrap()]
            .address
            .is_ipv6()
    );
    key(&mut application, K::Enter);
    let mut snapshot = application.snapshot.clone();
    snapshot.processes[0].identity.started += 1;
    application.replace(snapshot);
    assert!(application.detail().is_none());
    key(&mut application, K::Char('x'));
    assert!(application.pending.is_empty());
}

#[test]
fn port_screens_render_at_all_supported_sizes() {
    for (width, height) in [(28, 18), (48, 20), (80, 24), (120, 30), (160, 40)] {
        let mut application = ports_app();
        key(&mut application, K::Char('3'));
        for details in [false, true] {
            if details {
                key(&mut application, K::Enter);
            }
            let mut terminal =
                ratatui::Terminal::new(ratatui::backend::TestBackend::new(width, height)).unwrap();
            terminal
                .draw(|frame| view::draw(frame, &mut application))
                .unwrap();
            let text = terminal
                .backend()
                .buffer()
                .content
                .iter()
                .map(|cell| cell.symbol())
                .collect::<String>();
            assert!(text.contains("3000"));
            if width == 160 || width == 48 {
                let name = match (details, width) {
                    (false, 160) => "ports",
                    (true, 160) => "port-details",
                    (false, _) => "ports-compact",
                    (true, _) => "port-details-compact",
                };
                check_port_snapshot(name, terminal.backend().buffer());
            }
            if width == 160 {
                export_visual(
                    if details { "port-details" } else { "ports" },
                    terminal.backend().buffer(),
                );
            }
            if width == 48 {
                export_visual(
                    if details {
                        "port-details-compact"
                    } else {
                        "ports-compact"
                    },
                    terminal.backend().buffer(),
                );
            }
        }
    }
}

fn check_port_snapshot(name: &str, buffer: &ratatui::buffer::Buffer) {
    let text = buffer
        .content
        .chunks(buffer.area.width as usize)
        .map(|row| {
            let line = row.iter().map(|cell| cell.symbol()).collect::<String>();
            format!("{}\n", line.trim_end())
        })
        .collect::<String>();
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(format!("tests/snapshots/{name}.txt"));
    if std::env::var_os("OFLH_UPDATE_SNAPSHOTS").is_some() {
        std::fs::write(&path, &text).unwrap();
    }
    assert_eq!(std::fs::read_to_string(path).unwrap(), text, "{name}");
}
