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
    identity: Identity,
    name: String,
    parent: u32,
    ticks: u64,
    rss: Option<u64>,
}
fn stat(pid: u32) -> Result<Stat> {
    let bytes = fs::read(format!("/proc/{pid}/stat"))
        .map_err(|error| io("read process identity", error))?;
    parse_stat(pid, &bytes).ok_or(Error::Changed)
}
fn parse_stat(pid: u32, bytes: &[u8]) -> Option<Stat> {
    let start = bytes.iter().position(|bytes| *bytes == b'(')?;
    let end = bytes.iter().rposition(|bytes| *bytes == b')')?;
    if end <= start {
        return None;
    }
    let mut field_iter = bytes[end + 1..]
        .split(|bytes| bytes.is_ascii_whitespace())
        .filter(|fields| !fields.is_empty());
    let mut fields = [&b""[..]; 22];
    for field in &mut fields {
        *field = field_iter.next()?
    }
    let parse_number = |i| std::str::from_utf8(fields[i]).ok()?.parse::<u64>().ok();
    // SAFETY: sysconf takes a constant selector and no pointers.
    let page = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };
    Some(Stat {
        identity: Identity {
            pid,
            started: parse_number(19)?,
            started_sub: 0,
        },
        name: String::from_utf8_lossy(&bytes[start + 1..end]).into_owned(),
        parent: parse_number(1)?.try_into().ok()?,
        ticks: parse_number(11)?.checked_add(parse_number(12)?)?,
        rss: parse_number(21)
            .and_then(|parse_number| parse_number.checked_mul(page.try_into().ok()?)),
    })
}
fn total_cpu() -> u64 {
    fs::read_to_string("/proc/stat")
        .ok()
        .and_then(|s| {
            let mut fields = s.lines().next()?.split_whitespace();
            if fields.next() != Some("cpu") {
                return None;
            }
            fields.take(8).try_fold(0u64, |sum, parse_number| {
                sum.checked_add(parse_number.parse().ok()?)
            })
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
fn permission(error: &std::io::Error, restricted: &mut bool) {
    if error.kind() == std::io::ErrorKind::PermissionDenied {
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
        for entry in fs::read_dir("/proc").map_err(|error| io("enumerate processes", error))? {
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
            let stats = match stat(pid) {
                Ok(stats) => stats,
                Err(Error::Io { source, .. }) => {
                    if source.kind() == std::io::ErrorKind::PermissionDenied {
                        denied += 1
                    }
                    continue;
                }
                Err(_) => continue,
            };
            let base = entry.path();
            if let (Some(a), Ok(bytes)) = (&own_ns, fs::read_link(base.join("ns/mnt")))
                && *a != bytes
            {
                foreign += 1;
                continue;
            }
            let mut restricted = false;
            let mut process = Process {
                identity: stats.identity,
                name: stats.name,
                parent: stats.parent,
                memory: stats.rss,
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
                            process.cwd = path.clone()
                        } else {
                            process.executable = path.clone()
                        }
                        let (path, deleted) = deleted_path(&base, path);
                        if target.directory && !target.contains(&path) {
                            continue;
                        }
                        let metadata = if target.directory {
                            None
                        } else {
                            fs::metadata(reference).ok()
                        };
                        if target.matches(&path, metadata.as_ref()) {
                            process.usages.push(Usage {
                                path,
                                relation,
                                access,
                                deleted,
                                lock: None,
                            })
                        }
                    }
                    Err(error) => permission(&error, &mut restricted),
                }
            }
            match fs::read_dir(base.join("fd")) {
                Err(error) => permission(&error, &mut restricted),
                Ok(fds) => {
                    for descriptor in fds {
                        cancel.check()?;
                        let Ok(descriptor) = descriptor else { continue };
                        let reference = descriptor.path();
                        let path = match fs::read_link(&reference) {
                            Ok(process) => process,
                            Err(error) => {
                                permission(&error, &mut restricted);
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
                        let metadata = if target.directory {
                            None
                        } else {
                            fs::metadata(&reference).ok()
                        };
                        if !target.matches(&path, metadata.as_ref()) {
                            continue;
                        }
                        let descriptor_info = match fs::read_to_string(
                            base.join("fdinfo").join(descriptor.file_name()),
                        ) {
                            Ok(s) => s,
                            Err(error) => {
                                permission(&error, &mut restricted);
                                String::new()
                            }
                        };
                        let access = descriptor_info
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
                        process.usages.push(Usage {
                            path: path.clone(),
                            relation: Relation::Open,
                            access,
                            deleted,
                            lock: None,
                        });
                        for lock in locks(&descriptor_info) {
                            process.usages.push(Usage {
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
                Err(error) => permission(&error, &mut restricted),
                Ok(file) => {
                    let mut reader = BufReader::with_capacity(8192, file);
                    let mut line = Vec::with_capacity(512);
                    loop {
                        cancel.check()?;
                        line.clear();
                        match reader.read_until(b'\n', &mut line) {
                            Ok(0) => break,
                            Ok(_) => {}
                            Err(error) => {
                                permission(&error, &mut restricted);
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
                            process.usages.push(Usage {
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
            if !process.usages.is_empty() && stat(pid).is_ok_and(|s| s.identity == process.identity)
            {
                let uid = fs::read_to_string(base.join("status")).ok().and_then(|s| {
                    s.lines()
                        .find_map(|l| l.strip_prefix("Uid:"))
                        .and_then(|s| s.split_whitespace().nth(1))
                        .and_then(|s| s.parse::<u32>().ok())
                });
                process.user = uid
                    .map(|uid| {
                        users
                            .entry(uid)
                            .or_insert_with(|| super::unix::username(uid))
                            .clone()
                    })
                    .unwrap_or_else(|| "unknown".into());
                let mut next = process.parent;
                while next > 0
                    && process.ancestors.len() < 8
                    && next != pid
                    && !process.ancestors.iter().any(|a| a.identity.pid == next)
                {
                    cancel.check()?;
                    match stat(next) {
                        Ok(s) => {
                            process.ancestors.push(Ancestor {
                                identity: s.identity,
                                name: s.name,
                            });
                            next = s.parent
                        }
                        Err(_) => {
                            process.ancestors.push(Ancestor {
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
                snapshot.processes.push(process);
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
            .map(|process| process.identity)
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
        for &identity in ids {
            cancel.check()?;
            if let Ok(stats) = stat(identity.pid)
                && stats.identity == identity
            {
                raw.push((identity, stats.ticks, stats.rss))
            }
        }
        if total == 0 {
            return Ok(raw
                .into_iter()
                .map(|(identity, _, memory)| (identity, Metrics { memory, cpu: None }))
                .collect());
        }
        Ok(self.sampler.sample(raw, total))
    }
    fn terminate(&mut self, identity: Identity, force: bool, cancel: &Cancellation) -> Result<()> {
        identity.validate()?;
        cancel.check()?;
        let pid = Pid::from_raw(identity.pid.try_into().map_err(|_| Error::Protected)?)
            .ok_or(Error::Protected)?;
        let descriptor = pidfd_open(pid, PidfdFlags::empty())
            .map_err(|error| io("open process safely (requires Linux 5.3+)", error.into()))?;
        if stat(identity.pid)?.identity != identity {
            return Err(Error::Changed);
        }
        cancel.check()?;
        pidfd_send_signal(&descriptor, if force { Signal::KILL } else { Signal::TERM })
            .map_err(|error| io("signal process", error.into()))
    }
}
fn locks(descriptor_info: &str) -> Vec<String> {
    descriptor_info
        .lines()
        .filter_map(|line| {
            let fields: Vec<_> = line.split_whitespace().collect();
            if fields.len() != 9
                || fields[0] != "lock:"
                || !matches!(fields[2], "FLOCK" | "POSIX" | "OFDLCK")
                || !matches!(fields[4], "READ" | "WRITE")
            {
                return None;
            }
            Some(format!(
                "{} {} {} bytes {}–{}",
                fields[2], fields[3], fields[4], fields[7], fields[8]
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
        while line.get(at).is_some_and(|bytes| *bytes != b' ') {
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
        let (process, _, _, ino) =
            mapping(b"100-200 rw-p 0000 00:13 12  /a file\\012name\n").unwrap();
        assert_eq!(process, Path::new("/a file\nname"));
        assert_eq!(ino, 12);
    }
}
