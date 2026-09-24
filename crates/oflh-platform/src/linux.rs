use super::*;
use rustix::process::{Pid, PidfdFlags, Signal, pidfd_open, pidfd_send_signal};
use std::{
    fs::{self, File},
    io::{BufRead, BufReader},
    os::unix::{ffi::OsStrExt, fs::MetadataExt},
    path::{Path, PathBuf},
};
#[derive(Default)]
pub struct Native {
    sampler: Sampler,
}
#[derive(Debug)]
struct Stat {
    id: Identity,
    name: String,
    parent: u32,
    ticks: u64,
    rss: Option<u64>,
}
fn stat(pid: u32) -> Result<Stat> {
    let b = fs::read(format!("/proc/{pid}/stat")).map_err(|e| io("read process identity", e))?;
    parse_stat(pid, &b).ok_or(Error::Changed)
}
fn parse_stat(pid: u32, b: &[u8]) -> Option<Stat> {
    let start = b.iter().position(|b| *b == b'(')?;
    let end = b.iter().rposition(|b| *b == b')')?;
    if end <= start {
        return None;
    }
    let mut f = b[end + 1..]
        .split(|b| b.is_ascii_whitespace())
        .filter(|f| !f.is_empty());
    let mut fields = [&b""[..]; 22];
    for v in &mut fields {
        *v = f.next()?
    }
    let n = |i| std::str::from_utf8(fields[i]).ok()?.parse::<u64>().ok();
    // SAFETY: sysconf takes a constant selector and no pointers.
    let page = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };
    Some(Stat {
        id: Identity {
            pid,
            started: n(19)?,
            started_sub: 0,
        },
        name: String::from_utf8_lossy(&b[start + 1..end]).into_owned(),
        parent: n(1)?.try_into().ok()?,
        ticks: n(11)?.checked_add(n(12)?)?,
        rss: n(21).and_then(|n| n.checked_mul(page.try_into().ok()?)),
    })
}
fn total_cpu() -> u64 {
    fs::read_to_string("/proc/stat")
        .ok()
        .and_then(|s| {
            let mut f = s.lines().next()?.split_whitespace();
            if f.next() != Some("cpu") {
                return None;
            }
            f.take(8)
                .try_fold(0u64, |sum, n| sum.checked_add(n.parse().ok()?))
        })
        .unwrap_or(0)
}
fn deleted_path(base: &Path, mut path: PathBuf) -> (PathBuf, bool) {
    let raw = path.as_os_str().as_bytes();
    if let Some(stripped) = raw.strip_suffix(b" (deleted)")
        && fs::metadata(
            base.join("root")
                .join(path.strip_prefix("/").unwrap_or(&path)),
        )
        .is_err()
    {
        path = PathBuf::from(std::ffi::OsStr::from_bytes(stripped));
        return (path, true);
    }
    (path, false)
}
fn permission(e: &std::io::Error, restricted: &mut bool) {
    if e.kind() == std::io::ErrorKind::PermissionDenied {
        *restricted = true
    }
}
impl Backend for Native {
    fn scan(&mut self, target: &Target, cancel: &Cancellation) -> Result<Snapshot> {
        let mut snapshot = Snapshot::default();
        let mut denied = 0;
        let mut foreign = 0;
        let mut users = HashMap::new();
        let own_ns = fs::read_link("/proc/self/ns/mnt").ok();
        for entry in fs::read_dir("/proc").map_err(|e| io("enumerate processes", e))? {
            cancel.check()?;
            let Ok(entry) = entry else { continue };
            let Some(pid) = entry
                .file_name()
                .to_str()
                .and_then(|s| s.parse::<u32>().ok())
            else {
                continue;
            };
            if pid == std::process::id() {
                continue;
            }
            let st = match stat(pid) {
                Ok(st) => st,
                Err(Error::Io { source, .. }) => {
                    if source.kind() == std::io::ErrorKind::PermissionDenied {
                        denied += 1
                    }
                    continue;
                }
                Err(_) => continue,
            };
            let base = entry.path();
            if let (Some(a), Ok(b)) = (&own_ns, fs::read_link(base.join("ns/mnt")))
                && *a != b
            {
                foreign += 1;
                continue;
            }
            let mut restricted = false;
            let mut p = Process {
                identity: st.id,
                name: st.name,
                parent: st.parent,
                memory: st.rss,
                ..Process::default()
            };
            for (name, relation, access) in [
                ("cwd", Relation::Cwd, Access::Directory),
                ("exe", Relation::Executable, Access::Execute),
            ] {
                let reference = base.join(name);
                match fs::read_link(&reference) {
                    Ok(path) => {
                        if name == "cwd" {
                            p.cwd = path.clone()
                        } else {
                            p.executable = path.clone()
                        }
                        let (path, deleted) = deleted_path(&base, path);
                        if target.directory && !target.contains(&path) {
                            continue;
                        }
                        let meta = if target.directory {
                            None
                        } else {
                            fs::metadata(reference).ok()
                        };
                        if target.matches(&path, meta.as_ref()) {
                            p.usages.push(Usage {
                                path,
                                relation,
                                access,
                                deleted,
                                lock: None,
                            })
                        }
                    }
                    Err(e) => permission(&e, &mut restricted),
                }
            }
            match fs::read_dir(base.join("fd")) {
                Err(e) => permission(&e, &mut restricted),
                Ok(fds) => {
                    for fd in fds {
                        cancel.check()?;
                        let Ok(fd) = fd else { continue };
                        let reference = fd.path();
                        let path = match fs::read_link(&reference) {
                            Ok(p) => p,
                            Err(e) => {
                                permission(&e, &mut restricted);
                                continue;
                            }
                        };
                        if !path.is_absolute() {
                            continue;
                        }
                        let (path, deleted) = deleted_path(&base, path);
                        if target.directory && !target.contains(&path) {
                            continue;
                        }
                        let meta = if target.directory {
                            None
                        } else {
                            fs::metadata(&reference).ok()
                        };
                        if !target.matches(&path, meta.as_ref()) {
                            continue;
                        }
                        let info =
                            match fs::read_to_string(base.join("fdinfo").join(fd.file_name())) {
                                Ok(s) => s,
                                Err(e) => {
                                    permission(&e, &mut restricted);
                                    String::new()
                                }
                            };
                        let access = info
                            .lines()
                            .find_map(|line| line.strip_prefix("flags:"))
                            .and_then(|s| u32::from_str_radix(s.trim(), 8).ok())
                            .map(|flags| {
                                if flags & libc::O_PATH as u32 != 0 {
                                    Access::Reference
                                } else {
                                    match flags & libc::O_ACCMODE as u32 {
                                        0 => Access::Read,
                                        1 => Access::Write,
                                        2 => Access::ReadWrite,
                                        _ => Access::Unknown,
                                    }
                                }
                            })
                            .unwrap_or_default();
                        p.usages.push(Usage {
                            path: path.clone(),
                            relation: Relation::Open,
                            access,
                            deleted,
                            lock: None,
                        });
                        for lock in locks(&info) {
                            p.usages.push(Usage {
                                path: path.clone(),
                                relation: Relation::Locked,
                                access,
                                deleted,
                                lock: Some(LockEvidence::Kernel(lock)),
                            })
                        }
                    }
                }
            }
            match File::open(base.join("maps")) {
                Err(e) => permission(&e, &mut restricted),
                Ok(file) => {
                    let mut reader = BufReader::with_capacity(8192, file);
                    let mut line = Vec::with_capacity(512);
                    loop {
                        cancel.check()?;
                        line.clear();
                        match reader.read_until(b'\n', &mut line) {
                            Ok(0) => break,
                            Ok(_) => {}
                            Err(e) => {
                                permission(&e, &mut restricted);
                                break;
                            }
                        }
                        if let Some((path, access, dev, ino)) = mapping(&line) {
                            let (path, deleted) = deleted_path(&base, path);
                            if target.directory {
                                if !target.contains(&path) {
                                    continue;
                                }
                            } else if let Some(m) = &target.metadata {
                                if m.ino() != ino || m.dev() != dev {
                                    continue;
                                }
                            } else if !target.matches(&path, None) {
                                continue;
                            }
                            p.usages.push(Usage {
                                path,
                                relation: Relation::Mapped,
                                access,
                                deleted,
                                lock: None,
                            });
                        }
                    }
                }
            }
            if restricted {
                denied += 1
            }
            if !p.usages.is_empty() && stat(pid).is_ok_and(|s| s.id == p.identity) {
                let uid = fs::read_to_string(base.join("status")).ok().and_then(|s| {
                    s.lines()
                        .find_map(|l| l.strip_prefix("Uid:"))
                        .and_then(|s| s.split_whitespace().nth(1))
                        .and_then(|s| s.parse::<u32>().ok())
                });
                p.user = uid
                    .map(|uid| {
                        users
                            .entry(uid)
                            .or_insert_with(|| super::unix::username(uid))
                            .clone()
                    })
                    .unwrap_or_else(|| "unknown".into());
                let mut next = p.parent;
                while next > 0
                    && p.ancestors.len() < 8
                    && next != pid
                    && !p.ancestors.iter().any(|a| a.identity.pid == next)
                {
                    cancel.check()?;
                    match stat(next) {
                        Ok(s) => {
                            p.ancestors.push(Ancestor {
                                identity: s.id,
                                name: s.name,
                            });
                            next = s.parent
                        }
                        Err(_) => {
                            p.ancestors.push(Ancestor {
                                identity: Identity {
                                    pid: next,
                                    ..Identity::default()
                                },
                                name: "unavailable".into(),
                            });
                            break;
                        }
                    }
                }
                snapshot.processes.push(p);
            }
        }
        if denied > 0 {
            snapshot.warnings.push(format!("Limited visibility for {denied} processes (permissions). Elevated access may reveal more."))
        }
        if foreign > 0 {
            snapshot.warnings.push(format!("Skipped {foreign} processes in other mount namespaces; run oflh inside their container."))
        }
        snapshot.normalize();
        let ids = snapshot
            .processes
            .iter()
            .map(|p| p.identity)
            .collect::<Vec<_>>();
        apply_metrics(&mut snapshot, self.sample(&ids, cancel)?);
        Ok(snapshot)
    }
    fn sample(
        &mut self,
        ids: &[Identity],
        cancel: &Cancellation,
    ) -> Result<Vec<(Identity, Metrics)>> {
        let total = total_cpu();
        let mut raw = Vec::with_capacity(ids.len());
        for &id in ids {
            cancel.check()?;
            if let Ok(st) = stat(id.pid)
                && st.id == id
            {
                raw.push((id, st.ticks, st.rss))
            }
        }
        if total == 0 {
            return Ok(raw
                .into_iter()
                .map(|(id, _, memory)| (id, Metrics { memory, cpu: None }))
                .collect());
        }
        Ok(self.sampler.sample(raw, total))
    }
    fn terminate(&mut self, id: Identity, force: bool, cancel: &Cancellation) -> Result<()> {
        id.validate()?;
        cancel.check()?;
        let pid = Pid::from_raw(id.pid.try_into().map_err(|_| Error::Protected)?)
            .ok_or(Error::Protected)?;
        let fd = pidfd_open(pid, PidfdFlags::empty())
            .map_err(|e| io("open process safely (requires Linux 5.3+)", e.into()))?;
        if stat(id.pid)?.id != id {
            return Err(Error::Changed);
        }
        cancel.check()?;
        pidfd_send_signal(&fd, if force { Signal::KILL } else { Signal::TERM })
            .map_err(|e| io("signal process", e.into()))
    }
}
fn locks(info: &str) -> Vec<String> {
    info.lines()
        .filter_map(|line| {
            let f: Vec<_> = line.split_whitespace().collect();
            if f.len() != 9
                || f[0] != "lock:"
                || !matches!(f[2], "FLOCK" | "POSIX" | "OFDLCK")
                || !matches!(f[4], "READ" | "WRITE")
            {
                return None;
            }
            Some(format!(
                "{} {} {} bytes {}–{}",
                f[2], f[3], f[4], f[7], f[8]
            ))
        })
        .collect()
}
fn mapping(line: &[u8]) -> Option<(PathBuf, Access, u64, u64)> {
    let mut at = 0;
    let mut fields = [&b""[..]; 5];
    for field in &mut fields {
        while line.get(at) == Some(&b' ') {
            at += 1
        }
        let start = at;
        while line.get(at).is_some_and(|b| *b != b' ') {
            at += 1
        }
        *field = line.get(start..at)?;
    }
    while line.get(at) == Some(&b' ') {
        at += 1
    }
    let path = line.get(at..)?.strip_suffix(b"\n").unwrap_or(&line[at..]);
    if !path.starts_with(b"/") {
        return None;
    }
    let mut decoded = Vec::with_capacity(path.len());
    let mut i = 0;
    while i < path.len() {
        if path[i..].starts_with(b"\\012") {
            decoded.push(b'\n');
            i += 4
        } else {
            decoded.push(path[i]);
            i += 1
        }
    }
    let dev = std::str::from_utf8(fields[3]).ok()?;
    let (major, minor) = dev.split_once(':')?;
    let dev = libc::makedev(
        u32::from_str_radix(major, 16).ok()?,
        u32::from_str_radix(minor, 16).ok()?,
    );
    let ino = std::str::from_utf8(fields[4]).ok()?.parse().ok()?;
    let perm = fields[1];
    let access = if perm.contains(&b'x') {
        Access::Execute
    } else if perm.starts_with(b"rw") {
        Access::ReadWrite
    } else if perm.starts_with(b"r") {
        Access::Read
    } else if perm.get(1) == Some(&b'w') {
        Access::Write
    } else {
        Access::Mapped
    };
    Some((
        PathBuf::from(std::ffi::OsStr::from_bytes(&decoded)),
        access,
        dev,
        ino,
    ))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn held_locks_only() {
        assert_eq!(
            locks(
                "lock: 1: -> POSIX ADVISORY WRITE 42 00:13:12 0 EOF\nlock: 2: LEASE ACTIVE READ 42 00:13:12 0 EOF\nlock: 3: OFDLCK ADVISORY READ -1 00:13:12 4 8\n"
            ),
            ["OFDLCK ADVISORY READ bytes 4–8"]
        );
    }
    #[test]
    fn spaces_in_maps() {
        let (p, _, _, ino) = mapping(b"100-200 rw-p 0000 00:13 12  /a file\\012name\n").unwrap();
        assert_eq!(p, Path::new("/a file\nname"));
        assert_eq!(ino, 12);
    }
}
