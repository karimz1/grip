use super::*;
use std::{
    collections::HashSet,
    fs::{self, OpenOptions},
    mem::{MaybeUninit, size_of},
    os::{
        fd::AsRawFd,
        unix::{
            ffi::OsStrExt,
            fs::{MetadataExt, OpenOptionsExt},
        },
    },
    path::PathBuf,
};
// Missing libc records from Apple's public sys/proc_info.h. All fields are C integers
// or libc records; layout assertions below cover both supported 64-bit ABIs.
#[repr(C)]
struct FileInfo {
    flags: u32,
    status: u32,
    offset: i64,
    kind: i32,
    guard: u32,
}
#[repr(C)]
struct VnodeFd {
    file: FileInfo,
    vnode: libc::vnode_info_path,
}
#[repr(C)]
struct RegionInfo {
    protection: u32,
    max_protection: u32,
    inheritance: u32,
    flags: u32,
    offset: u64,
    behavior: u32,
    wired: u32,
    tag: u32,
    resident: u32,
    private_now: u32,
    swapped: u32,
    dirty: u32,
    refs: u32,
    shadow: u32,
    share_mode: u32,
    private_resident: u32,
    shared_resident: u32,
    object: u32,
    depth: u32,
    address: u64,
    size: u64,
}
#[repr(C)]
struct Region {
    info: RegionInfo,
    vnode: libc::vnode_info_path,
}
const _: () = {
    assert!(size_of::<libc::proc_bsdinfo>() == 136);
    assert!(size_of::<VnodeFd>() == 1200);
    assert!(size_of::<Region>() == 1272);
    assert!(size_of::<libc::proc_vnodepathinfo>() == 2352);
};
// Private implementations restrict the generic native reader to POD layouts.
trait Info {
    const FLAVOR: i32;
}
impl Info for libc::proc_bsdinfo {
    const FLAVOR: i32 = 3;
}
impl Info for libc::proc_vnodepathinfo {
    const FLAVOR: i32 = 9;
}
impl Info for Region {
    const FLAVOR: i32 = 8;
}
fn info<T: Info>(pid: u32, arg: u64) -> Result<T> {
    let mut out = MaybeUninit::<T>::zeroed();
    // SAFETY: Info is private and implemented only for the matching POD C layouts above.
    let n = unsafe {
        libc::proc_pidinfo(
            pid as i32,
            T::FLAVOR,
            arg,
            out.as_mut_ptr().cast(),
            size_of::<T>() as i32,
        )
    };
    if n != size_of::<T>() as i32 {
        return Err(io("inspect process", std::io::Error::last_os_error()));
    }
    // SAFETY: a full record was returned, and all bit patterns of the POD fields are valid.
    Ok(unsafe { out.assume_init() })
}
fn bytes(chars: &[i8]) -> Vec<u8> {
    chars
        .iter()
        .take_while(|&&c| c != 0)
        .map(|&c| c as u8)
        .collect()
}
fn vnode_path(v: &libc::vnode_info_path) -> PathBuf {
    let b: Vec<u8> = v
        .vip_path
        .iter()
        .flatten()
        .take_while(|&&c| c != 0)
        .map(|&c| c as u8)
        .collect();
    PathBuf::from(std::ffi::OsStr::from_bytes(&b))
}
fn process(pid: u32) -> Result<(Process, u32)> {
    let b: libc::proc_bsdinfo = info(pid, 0)?;
    let name = bytes(if b.pbi_name[0] == 0 {
        &b.pbi_comm
    } else {
        &b.pbi_name
    });
    let mut path = [0u8; 4096];
    // SAFETY: writable buffer of the supplied length; proc_pidpath returns a bounded C string.
    let n = unsafe { libc::proc_pidpath(pid as i32, path.as_mut_ptr().cast(), path.len() as u32) };
    let executable = if n > 0 {
        let end = path.iter().position(|b| *b == 0).unwrap_or(path.len());
        PathBuf::from(std::ffi::OsStr::from_bytes(&path[..end]))
    } else {
        PathBuf::new()
    };
    Ok((
        Process {
            identity: Identity {
                pid,
                started: b.pbi_start_tvsec,
                started_sub: b.pbi_start_tvusec,
            },
            name: String::from_utf8_lossy(&name).into_owned(),
            parent: b.pbi_ppid,
            executable,
            ..Process::default()
        },
        b.pbi_uid,
    ))
}
fn identity(pid: u32) -> Result<Identity> {
    let b: libc::proc_bsdinfo = info(pid, 0)?;
    Ok(Identity {
        pid,
        started: b.pbi_start_tvsec,
        started_sub: b.pbi_start_tvusec,
    })
}
fn add(p: &mut Process, target: &Target, path: PathBuf, relation: Relation, access: Access) {
    if path.as_os_str().is_empty() || target.directory && !target.contains(&path) {
        return;
    }
    let meta = if target.directory {
        None
    } else {
        fs::metadata(&path).ok()
    };
    if target.matches(&path, meta.as_ref()) {
        p.usages.push(Usage {
            path,
            relation,
            access,
            ..Usage::default()
        })
    }
}
#[derive(Default)]
pub struct Native {
    sampler: Sampler,
}
impl Backend for Native {
    fn scan(&mut self, target: &Target, cancel: &Cancellation) -> Result<Snapshot> {
        cancel.check()?;
        // SAFETY: null/zero requests a count only.
        let count = unsafe { libc::proc_listallpids(std::ptr::null_mut(), 0) };
        if count <= 0 {
            return Err(io("enumerate processes", std::io::Error::last_os_error()));
        }
        let mut pids = vec![0i32; count as usize + 256];
        loop {
            cancel.check()?;
            // SAFETY: initialized integer vector is writable for exactly the supplied byte size.
            let n = unsafe {
                libc::proc_listallpids(
                    pids.as_mut_ptr().cast(),
                    (pids.len() * size_of::<i32>()) as i32,
                )
            };
            if n < 0 {
                return Err(io("enumerate processes", std::io::Error::last_os_error()));
            }
            if (n as usize) < pids.len() {
                pids.truncate(n as usize);
                break;
            }
            if pids.len() > 1_000_000 {
                return Err(Error::Unavailable(
                    "process enumeration changed too quickly".into(),
                ));
            }
            pids.resize(pids.len() * 2, 0);
        }
        let mut snapshot=Snapshot{warnings:vec!["macOS: first POSIX conflict per readable file; flock-only locks and additional ranges may be missed.".into()],..Snapshot::default()};
        let mut limited = 0;
        let mut users = HashMap::new();
        for pid in pids {
            cancel.check()?;
            if pid <= 0 || pid as u32 == std::process::id() {
                continue;
            }
            let pid = pid as u32;
            let Ok((mut p, uid)) = process(pid) else {
                limited += 1;
                continue;
            };
            let mut partial = false;
            let exe = p.executable.clone();
            add(&mut p, target, exe, Relation::Executable, Access::Execute);
            if let Ok(cwd) = info::<libc::proc_vnodepathinfo>(pid, 0) {
                p.cwd = vnode_path(&cwd.pvi_cdir);
                let path = p.cwd.clone();
                add(&mut p, target, path, Relation::Cwd, Access::Directory)
            } else {
                partial = true
            }
            // SAFETY: null/zero requests the descriptor buffer size.
            let needed = unsafe { libc::proc_pidinfo(pid as i32, 1, 0, std::ptr::null_mut(), 0) };
            if needed > 0 && needed < 32 * 1024 * 1024 {
                let len = needed as usize / size_of::<libc::proc_fdinfo>() + 128;
                let mut fds = Vec::<libc::proc_fdinfo>::with_capacity(len);
                // SAFETY: spare capacity provides len correctly aligned writable records. Length stays zero until validated.
                let n = unsafe {
                    libc::proc_pidinfo(
                        pid as i32,
                        1,
                        0,
                        fds.as_mut_ptr().cast(),
                        (len * size_of::<libc::proc_fdinfo>()) as i32,
                    )
                };
                if n <= 0 {
                    partial = true
                } else if n as usize > len * size_of::<libc::proc_fdinfo>() {
                    return Err(Error::Unavailable(
                        "invalid libproc descriptor length".into(),
                    ));
                } else {
                    if n as usize == len * size_of::<libc::proc_fdinfo>() {
                        partial = true
                    }
                    // SAFETY: native call initialized exactly the complete records covered by n bytes.
                    unsafe { fds.set_len(n as usize / size_of::<libc::proc_fdinfo>()) };
                    for fd in fds {
                        cancel.check()?;
                        if fd.proc_fdtype != 1 {
                            continue;
                        }
                        let mut vnode = MaybeUninit::<VnodeFd>::zeroed();
                        // SAFETY: flavor 2 writes the VnodeFd POD layout into an exact-sized buffer.
                        let n = unsafe {
                            libc::proc_pidfdinfo(
                                pid as i32,
                                fd.proc_fd,
                                2,
                                vnode.as_mut_ptr().cast(),
                                size_of::<VnodeFd>() as i32,
                            )
                        };
                        if n != size_of::<VnodeFd>() as i32 {
                            partial = true;
                            continue;
                        }
                        // SAFETY: complete POD record was returned.
                        let vnode = unsafe { vnode.assume_init() };
                        let access = match vnode.file.flags & 3 {
                            1 => Access::Read,
                            2 => Access::Write,
                            3 => Access::ReadWrite,
                            _ => Access::Unknown,
                        };
                        add(
                            &mut p,
                            target,
                            vnode_path(&vnode.vnode),
                            Relation::Open,
                            access,
                        );
                    }
                }
            } else if needed < 0 {
                partial = true
            }
            let mut address = 0;
            for step in 0..65536 {
                cancel.check()?;
                let Ok(region) = info::<Region>(pid, address) else {
                    break;
                };
                let flags = region.info.protection;
                let access = if flags & 4 != 0 {
                    Access::Execute
                } else {
                    match flags & 3 {
                        1 => Access::Read,
                        2 => Access::Write,
                        3 => Access::ReadWrite,
                        _ => Access::Mapped,
                    }
                };
                add(
                    &mut p,
                    target,
                    vnode_path(&region.vnode),
                    Relation::Mapped,
                    access,
                );
                let Some(next) = region.info.address.checked_add(region.info.size) else {
                    partial = true;
                    break;
                };
                if next <= address {
                    break;
                }
                address = next;
                if step == 65535 {
                    partial = true
                }
            }
            if partial {
                limited += 1
            }
            if !p.usages.is_empty() && identity(pid).is_ok_and(|id| id == p.identity) {
                p.user = users
                    .entry(uid)
                    .or_insert_with(|| super::unix::username(uid))
                    .clone();
                let mut parent = p.parent;
                while parent > 0
                    && parent != pid
                    && p.ancestors.len() < 8
                    && !p.ancestors.iter().any(|a| a.identity.pid == parent)
                {
                    cancel.check()?;
                    match process(parent) {
                        Ok((a, _)) => {
                            p.ancestors.push(Ancestor {
                                identity: a.identity,
                                name: a.name,
                            });
                            parent = a.parent
                        }
                        Err(_) => {
                            p.ancestors.push(Ancestor {
                                identity: Identity {
                                    pid: parent,
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
        if limited > 0 {
            snapshot.warnings.push(format!(
                "{limited} processes could not be fully inspected (permissions or process changes)."
            ))
        }
        detect_locks(&mut snapshot, cancel)?;
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
        let mut raw = Vec::with_capacity(ids.len());
        for &id in ids {
            cancel.check()?;
            if !identity(id.pid).is_ok_and(|i| i == id) {
                continue;
            }
            let mut usage = MaybeUninit::<libc::rusage_info_v0>::zeroed();
            // SAFETY: flavor zero expects rusage_info_v0 storage, despite the void** typedef in Apple's API.
            let code =
                unsafe { libc::proc_pid_rusage(id.pid as i32, 0, usage.as_mut_ptr().cast()) };
            if code != 0 || !identity(id.pid).is_ok_and(|i| i == id) {
                continue;
            }
            // SAFETY: successful call initialized the POD structure.
            let usage = unsafe { usage.assume_init() };
            raw.push((
                id,
                usage.ri_user_time.saturating_add(usage.ri_system_time),
                Some(usage.ri_resident_size),
            ));
        }
        let total = self
            .sampler
            .clock()
            .saturating_mul(std::thread::available_parallelism().map_or(1, |n| n.get()) as u64);
        Ok(self.sampler.sample(raw, total))
    }
    fn terminate(&mut self, id: Identity, force: bool, cancel: &Cancellation) -> Result<()> {
        id.validate()?;
        cancel.check()?;
        if identity(id.pid)? != id {
            return Err(Error::Changed);
        }
        // SAFETY: positive validated PID and valid signal. macOS has no public pidfd; a residual race remains.
        if unsafe {
            libc::kill(
                id.pid as i32,
                if force { libc::SIGKILL } else { libc::SIGTERM },
            )
        } != 0
        {
            return Err(io("signal process", std::io::Error::last_os_error()));
        }
        Ok(())
    }
}
fn detect_locks(snapshot: &mut Snapshot, cancel: &Cancellation) -> Result<()> {
    let paths: HashSet<_> = snapshot
        .processes
        .iter()
        .flat_map(|p| p.usages.iter().map(|u| u.path.clone()))
        .collect();
    for path in paths {
        cancel.check()?;
        let Ok(meta) = fs::metadata(&path) else {
            continue;
        };
        if !meta.is_file() {
            continue;
        }
        let Ok(file) = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NONBLOCK | libc::O_CLOEXEC)
            .open(&path)
        else {
            continue;
        };
        let Ok(opened) = file.metadata() else {
            continue;
        };
        if meta.dev() != opened.dev() || meta.ino() != opened.ino() {
            continue;
        }
        let mut lock = libc::flock {
            l_start: 0,
            l_len: 0,
            l_pid: 0,
            l_type: libc::F_WRLCK,
            l_whence: libc::SEEK_SET as i16,
        };
        // SAFETY: F_GETLK queries existing locks using a valid fd and correctly initialized flock.
        if unsafe { libc::fcntl(file.as_raw_fd(), libc::F_GETLK, &mut lock) } != 0
            || lock.l_type == libc::F_UNLCK
            || lock.l_pid <= 0
        {
            continue;
        }
        if let Some(p) = snapshot
            .processes
            .iter_mut()
            .find(|p| p.identity.pid == lock.l_pid as u32)
        {
            if !identity(p.identity.pid).is_ok_and(|id| id == p.identity) {
                continue;
            }
            let access = if lock.l_type == libc::F_WRLCK {
                Access::Write
            } else {
                Access::Read
            };
            let end = if lock.l_len > 0 {
                lock.l_start.saturating_add(lock.l_len - 1).to_string()
            } else {
                "EOF".into()
            };
            p.usages.push(Usage {
                path,
                relation: Relation::Locked,
                access,
                lock: Some(LockEvidence::Kernel(format!(
                    "POSIX ADVISORY {} bytes {}–{end}",
                    access.label(),
                    lock.l_start
                ))),
                ..Usage::default()
            })
        }
    }
    Ok(())
}
