//! Exercise the shipped CLI in a real pseudo-terminal on all three platforms.
use portable_pty::{CommandBuilder, PtySize, native_pty_system};
use std::{
    io::{Read, Write},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
#[test]
fn native_terminal_workflow() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("terminal-fixture ü.bin");
    let mut file = std::fs::File::create(&path).unwrap();
    file.write_all(b"fixture").unwrap();
    let pair = native_pty_system()
        .openpty(PtySize {
            rows: 30,
            cols: 120,
            pixel_width: 0,
            pixel_height: 0,
        })
        .unwrap();
    let mut cmd = CommandBuilder::new(env!("CARGO_BIN_EXE_oflh"));
    cmd.arg(dir.path());
    cmd.env("TERM", "xterm-256color");
    cmd.env("COLORTERM", "truecolor");
    let mut child = pair.slave.spawn_command(cmd).unwrap();
    drop(pair.slave);
    let mut killer = child.clone_killer();
    struct Guard(Box<dyn portable_pty::ChildKiller + Send + Sync>);
    impl Drop for Guard {
        fn drop(&mut self) {
            let _ = self.0.kill();
        }
    }
    let _guard = Guard(child.clone_killer());
    let mut reader = pair.master.try_clone_reader().unwrap();
    let mut writer = pair.master.take_writer().unwrap();
    let parser = Arc::new(Mutex::new(vt100::Parser::new(30, 120, 0)));
    let output = parser.clone();
    std::thread::spawn(move || {
        let mut buf = [0; 8192];
        while let Ok(n) = reader.read(&mut buf) {
            if n == 0 {
                break;
            }
            output.lock().unwrap().process(&buf[..n]);
        }
    });
    let wait = |needle: &str| {
        let start = Instant::now();
        loop {
            let text = parser.lock().unwrap().screen().contents();
            if text.contains(needle) {
                break;
            }
            if start.elapsed() > Duration::from_secs(20) {
                panic!("did not render {needle:?}:\n{text}")
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    };
    wait("Processes");
    wait("terminal-fixture");
    writer.write_all(b"/terminal-fixture\r").unwrap();
    writer.flush().unwrap();
    wait("1 of");
    writer.write_all(b"\r").unwrap();
    writer.flush().unwrap();
    wait("process details");
    writer.write_all(b"l").unwrap();
    writer.flush().unwrap();
    wait("LOCKS ONLY");
    writer.write_all(b"l").unwrap();
    writer.flush().unwrap();
    wait("ALL USAGES");
    writer.write_all(b"r").unwrap();
    writer.flush().unwrap();
    std::thread::sleep(Duration::from_millis(100));
    pair.master
        .resize(PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        })
        .unwrap();
    parser.lock().unwrap().screen_mut().set_size(24, 80);
    writer.write_all(b"q?").unwrap();
    writer.flush().unwrap();
    wait("SCAN DETAILS");
    writer.write_all(b"q").unwrap();
    writer.flush().unwrap();
    wait("Processes");
    writer.write_all(b"q").unwrap();
    writer.flush().unwrap();
    let start = Instant::now();
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            assert!(status.success());
            break;
        }
        if start.elapsed() > Duration::from_secs(5) {
            let _ = killer.kill();
            panic!("quit remained blocked by scan")
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}
#[test]
fn cli_contract() {
    let bin = env!("CARGO_BIN_EXE_oflh");
    let version = std::process::Command::new(bin)
        .arg("--version")
        .output()
        .unwrap();
    assert!(version.status.success());
    assert!(String::from_utf8_lossy(&version.stdout).starts_with("oflh "));
    let help = std::process::Command::new(bin)
        .arg("--help")
        .output()
        .unwrap();
    assert!(help.status.success());
    let bad = std::process::Command::new(bin)
        .args(["one", "two"])
        .output()
        .unwrap();
    assert_eq!(bad.status.code(), Some(2));
    let piped = std::process::Command::new(bin).output().unwrap();
    assert_eq!(piped.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&piped.stderr).contains("interactive terminal"));
}
