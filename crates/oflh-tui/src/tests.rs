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
