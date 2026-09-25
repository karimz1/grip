//! Real OS contracts. Helpers exist only in this test executable, never in oflh.
use oflh_core::*;
use oflh_platform::{Backend, native};
use std::{
    fs::{self, OpenOptions},
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::mpsc,
    time::Duration,
};
struct ChildGuard(Child);
impl Drop for ChildGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
fn ready(child: &mut Child) -> u32 {
    let stdout = child.stdout.take().unwrap();
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines() {
            let Ok(line) = line else { break };
            if let Some(id) = line.strip_prefix("OFLH READY ") {
                let _ = tx.send(id.parse::<u32>().unwrap());
            }
            if line == "LOCK FIXTURE READY" {
                let _ = tx.send(0);
            }
        }
    });
    rx.recv_timeout(Duration::from_secs(15))
        .expect("helper readiness timeout")
}
fn start(path: &Path, mode: &str) -> ChildGuard {
    let mut c = ChildGuard(
        Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "fixture_helper", "--nocapture"])
            .env("OFLH_FIXTURE", path)
            .env("OFLH_MODE", mode)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap(),
    );
    ready(&mut c.0);
    c
}
fn process(backend: &mut dyn Backend, path: &Path, pid: u32) -> Process {
    let target = Target::new(path).unwrap();
    let result = backend.scan(&target, &Cancellation::default()).unwrap();
    result
        .processes
        .into_iter()
        .find(|p| p.identity.pid == pid)
        .unwrap_or_else(|| panic!("PID {pid} missing for {}", path.display()))
}
#[test]
fn fixture_helper() {
    let Some(path) = std::env::var_os("OFLH_FIXTURE") else {
        return;
    };
    let path = PathBuf::from(path);
    let mode = std::env::var("OFLH_MODE").unwrap();
    if mode == "parent" {
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "fixture_helper", "--nocapture"])
            .env("OFLH_MODE", "write")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        ready(&mut child);
        println!("OFLH READY {}", child.id());
        std::io::stdout().flush().unwrap();
        let mut line = String::new();
        let _ = std::io::stdin().read_line(&mut line);
        drop(child.stdin.take());
        let _ = child.wait();
        std::process::exit(0);
    }
    let mut options = OpenOptions::new();
    options.read(true).write(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        use windows_sys::Win32::Storage::FileSystem::*;
        let share = match mode.as_str() {
            "read" => FILE_SHARE_READ | FILE_SHARE_DELETE,
            "write" => FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            "range" => FILE_SHARE_READ | FILE_SHARE_WRITE,
            _ => FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
        };
        options.share_mode(share);
    }
    let file = options.open(&path).unwrap();
    std::env::set_current_dir(path.parent().unwrap()).unwrap();
    #[cfg(unix)]
    let mapping = {
        use std::os::fd::AsRawFd;
        // SAFETY: test file is at least 4096 bytes and fd lives until unmap.
        let p = unsafe {
            libc::mmap(
                std::ptr::null_mut(),
                4096,
                libc::PROT_READ,
                libc::MAP_PRIVATE,
                file.as_raw_fd(),
                0,
            )
        };
        assert_ne!(p, libc::MAP_FAILED);
        if mode != "open" {
            // SAFETY: zero is a valid initial POD flock; fields below specify a valid POSIX lock.
            let mut lock: libc::flock = unsafe { std::mem::zeroed() };
            lock.l_type = if mode == "read" {
                libc::F_RDLCK
            } else {
                libc::F_WRLCK
            } as _;
            lock.l_whence = libc::SEEK_SET as _;
            if mode == "range" {
                lock.l_start = 128;
                lock.l_len = 256;
            }
            assert_eq!(
                // SAFETY: valid file and writable initialized POSIX lock record.
                unsafe { libc::fcntl(file.as_raw_fd(), libc::F_SETLK, &lock) },
                0
            );
        }
        p
    };
    let running = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
    let busy = running.clone();
    let worker = std::thread::spawn(move || {
        let mut n = 1u64;
        while busy.load(std::sync::atomic::Ordering::Relaxed) {
            n = n.wrapping_mul(1664525).wrapping_add(1013904223);
            std::hint::black_box(n);
        }
    });
    println!("OFLH READY {}", std::process::id());
    std::io::stdout().flush().unwrap();
    let mut line = String::new();
    let n = std::io::stdin().read_line(&mut line).unwrap();
    #[cfg(unix)]
    {
        // SAFETY: mapping is the live address and length returned above, unmapped exactly once.
        assert_eq!(unsafe { libc::munmap(mapping, 4096) }, 0);
    }
    drop(file);
    running.store(false, std::sync::atomic::Ordering::Relaxed);
    worker.join().unwrap();
    if n != 0 {
        println!("RELEASED");
        std::io::stdout().flush().unwrap();
        let _ = std::io::stdin().read_line(&mut line);
    }
    std::process::exit(0);
}
#[test]
fn native_feature_contract() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("file ü with spaces.bin");
    fs::write(&path, vec![7; 4096]).unwrap();
    let mut child = start(&path, "write");
    let mut backend = native().unwrap();
    let p = process(&mut *backend, &path, child.0.id());
    assert!(p.usages.iter().any(|u| u.lock.is_some()), "{:?}", p.usages);
    assert!(p.memory.is_some_and(|m| m > 0));
    assert!(
        p.ancestors
            .iter()
            .any(|a| a.identity.pid == std::process::id() && a.identity.started != 0)
    );
    std::thread::sleep(Duration::from_millis(180));
    let m = backend
        .sample(&[p.identity], &Cancellation::default())
        .unwrap();
    assert!(
        m.iter()
            .any(|(id, m)| *id == p.identity && m.cpu.is_some_and(|n| n > 0.0 && n <= 100.0)),
        "{m:?}"
    );
    let mut stale = p.identity;
    stale.started += 1;
    assert!(matches!(
        backend.terminate(stale, true, &Cancellation::default()),
        Err(Error::Changed)
    ));
    #[cfg(unix)]
    {
        let p = process(&mut *backend, dir.path(), child.0.id());
        assert!(p.usages.iter().any(|u| u.relation == Relation::Cwd));
        assert!(p.usages.iter().any(|u| u.relation == Relation::Mapped));
    }
    backend
        .terminate(p.identity, true, &Cancellation::default())
        .unwrap();
    // Termination requests are asynchronous on Windows. Observe process exit before
    // expecting its restrictive file-sharing handles to have been released.
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while child.0.try_wait().unwrap().is_none() {
        assert!(
            std::time::Instant::now() < deadline,
            "helper did not exit after termination"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
    assert_eq!(fs::read(&path).unwrap(), vec![7; 4096]);
}
#[test]
fn native_lock_modes_and_release() {
    for mode in ["open", "read", "write", "range"] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("mode.bin");
        fs::write(&path, vec![0; 4096]).unwrap();
        let mut child = start(&path, mode);
        let mut backend = native().unwrap();
        let p = process(&mut *backend, &path, child.0.id());
        assert_eq!(
            p.usages.iter().any(|u| u.lock.is_some()),
            mode != "open",
            "mode {mode}: {:?}",
            p.usages
        );
        if mode != "open" {
            let expected = if cfg!(windows) {
                match mode {
                    "read" => "write denied",
                    "write" => "read denied",
                    _ => "delete denied",
                }
            } else {
                match mode {
                    "read" => "read",
                    "write" => "write",
                    _ => "128–383",
                }
            };
            assert!(
                p.usages
                    .iter()
                    .filter_map(|u| u.lock.as_ref())
                    .any(|l| l.to_string().to_lowercase().contains(expected))
            );
        }
        child.0.stdin.as_mut().unwrap().write_all(b"\n").unwrap();
        let target = Target::new(&path).unwrap();
        let until = std::time::Instant::now() + Duration::from_secs(5);
        loop {
            let r = backend.scan(&target, &Cancellation::default()).unwrap();
            if !r.processes.iter().any(|p| {
                p.identity.pid == child.0.id() && p.usages.iter().any(|u| u.lock.is_some())
            }) {
                break;
            }
            assert!(
                std::time::Instant::now() < until,
                "lock persisted after close"
            );
            std::thread::sleep(Duration::from_millis(30));
        }
        assert!(
            child.0.try_wait().unwrap().is_none(),
            "release should keep helper alive"
        );
    }
}
#[test]
fn native_cancellation_and_protection() {
    let mut b = native().unwrap();
    let c = Cancellation::default();
    c.cancel();
    assert!(matches!(
        b.scan(&Target::new(".").unwrap(), &c),
        Err(Error::Cancelled)
    ));
    for pid in [0, 1, std::process::id()] {
        assert!(matches!(
            b.terminate(
                Identity {
                    pid,
                    started: 1,
                    started_sub: 0
                },
                true,
                &Cancellation::default()
            ),
            Err(Error::Protected)
        ));
    }
}
#[test]
fn native_graceful() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("file");
    fs::write(&path, vec![0; 4096]).unwrap();
    let child = start(&path, "open");
    let mut b = native().unwrap();
    let p = process(&mut *b, &path, child.0.id());
    let result = b.terminate(p.identity, false, &Cancellation::default());
    #[cfg(windows)]
    assert!(result.is_err(), "console helper must not be force killed");
    #[cfg(unix)]
    {
        let mut child = child;
        result.unwrap();
        let until = std::time::Instant::now() + Duration::from_secs(5);
        while child.0.try_wait().unwrap().is_none() {
            assert!(std::time::Instant::now() < until);
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}
#[cfg(target_os = "linux")]
#[test]
fn deleted_replacement_and_hardlink() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("deleted file");
    let alias = dir.path().join("alias");
    fs::write(&path, vec![0; 4096]).unwrap();
    fs::hard_link(&path, &alias).unwrap();
    let child = start(&path, "open");
    let mut b = native().unwrap();
    process(&mut *b, &alias, child.0.id());
    fs::remove_file(&path).unwrap();
    let p = process(&mut *b, &path, child.0.id());
    assert!(p.usages.iter().any(|u| u.deleted));
    fs::write(&path, vec![1; 4096]).unwrap();
    let result = b
        .scan(&Target::new(&path).unwrap(), &Cancellation::default())
        .unwrap();
    assert!(
        !result
            .processes
            .iter()
            .any(|p| p.identity.pid == child.0.id())
    );
    let p = process(&mut *b, dir.path(), child.0.id());
    assert!(p.usages.iter().any(|u| u.deleted));
}
#[test]
fn native_parent_termination() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("parent.bin");
    fs::write(&path, vec![0; 4096]).unwrap();
    let mut parent = ChildGuard(
        Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "fixture_helper", "--nocapture"])
            .env("OFLH_FIXTURE", &path)
            .env("OFLH_MODE", "parent")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap(),
    );
    let child_id = ready(&mut parent.0);
    let mut b = native().unwrap();
    let p = process(&mut *b, &path, child_id);
    let id = p
        .ancestors
        .iter()
        .find(|a| a.identity.pid == parent.0.id())
        .expect("parent missing")
        .identity;
    let mut stale = id;
    stale.started += 1;
    assert!(matches!(
        b.terminate(stale, true, &Cancellation::default()),
        Err(Error::Changed)
    ));
    b.terminate(id, true, &Cancellation::default()).unwrap();
    let until = std::time::Instant::now() + Duration::from_secs(10);
    loop {
        let r = b
            .scan(&Target::new(&path).unwrap(), &Cancellation::default())
            .unwrap();
        if !r
            .processes
            .iter()
            .any(|p| p.identity.pid == child_id && p.usages.iter().any(|u| u.lock.is_some()))
        {
            break;
        }
        assert!(
            std::time::Instant::now() < until,
            "child did not release on parent pipe close"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
}
#[test]
fn independent_c_fixture() {
    let dir = tempfile::tempdir().unwrap();
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/lock-fixture.c");
    let exe = dir.path().join(if cfg!(windows) {
        "fixture.exe"
    } else {
        "fixture"
    });
    let target = if cfg!(target_os = "linux") {
        if cfg!(target_arch = "aarch64") {
            "aarch64-unknown-linux-gnu"
        } else {
            "x86_64-unknown-linux-gnu"
        }
    } else if cfg!(target_os = "macos") {
        if cfg!(target_arch = "aarch64") {
            "aarch64-apple-darwin"
        } else {
            "x86_64-apple-darwin"
        }
    } else if cfg!(target_arch = "aarch64") {
        "aarch64-pc-windows-msvc"
    } else {
        "x86_64-pc-windows-msvc"
    };
    let compiler = cc::Build::new()
        .target(target)
        .host(target)
        .opt_level(2)
        .cargo_metadata(false)
        .get_compiler();
    let mut command = compiler.to_command();
    command.current_dir(dir.path());
    if compiler.is_like_msvc() {
        command.arg(&source).arg(format!("/Fe:{}", exe.display()));
    } else {
        command.arg(&source).arg("-o").arg(&exe);
    }
    assert!(
        command
            .status()
            .expect("C compiler required for independent interoperability test")
            .success()
    );
    for mode in ["open", "read", "write", "range"] {
        let path = dir.path().join("external.dat");
        fs::write(&path, vec![0; 4096]).unwrap();
        let mut c = Command::new(&exe);
        c.arg(mode).arg(&path);
        if mode == "range" {
            c.args(["128", "256"]);
        }
        let mut child = ChildGuard(
            c.stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::inherit())
                .spawn()
                .unwrap(),
        );
        ready(&mut child.0);
        let mut b = native().unwrap();
        let p = process(&mut *b, &path, child.0.id());
        assert_eq!(
            p.usages.iter().any(|u| u.lock.is_some()),
            mode != "open",
            "external {mode}: {:?}",
            p.usages
        );
    }
}
