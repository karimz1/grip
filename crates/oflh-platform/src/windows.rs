use super::*;
use std::{
    ffi::OsString,
    mem::size_of,
    os::windows::ffi::{OsStrExt, OsStringExt},
    path::{Path, PathBuf},
    ptr::{null, null_mut},
};
use windows_sys::Win32::{
    Foundation::*,
    Security::*,
    Storage::FileSystem::*,
    System::{Diagnostics::ToolHelp::*, ProcessStatus::*, RestartManager::*, Threading::*},
    UI::WindowsAndMessaging::*,
};
struct Handle(HANDLE);
impl Handle {
    fn new(h: HANDLE, operation: &'static str) -> Result<Self> {
        if h.is_null() || h == INVALID_HANDLE_VALUE {
            Err(io(operation, std::io::Error::last_os_error()))
        } else {
            Ok(Self(h))
        }
    }
}
impl Drop for Handle {
    fn drop(&mut self) {
        // SAFETY: Handle owns exactly one valid handle and never exposes ownership.
        unsafe { CloseHandle(self.0) };
    }
}
fn open(pid: u32, access: u32) -> Result<Handle> {
    // SAFETY: OpenProcess accepts numeric access and PID and returns a new owned handle.
    Handle::new(unsafe { OpenProcess(access, 0, pid) }, "open process")
}
fn wide(p: &Path) -> Result<Vec<u16>> {
    let mut s: Vec<_> = p.as_os_str().encode_wide().collect();
    if s.contains(&0) {
        return Err(Error::Unavailable("path contains NUL".into()));
    }
    s.push(0);
    Ok(s)
}
fn path(s: &[u16]) -> PathBuf {
    PathBuf::from(OsString::from_wide(
        &s[..s.iter().position(|&c| c == 0).unwrap_or(s.len())],
    ))
}
fn ticks(t: FILETIME) -> u64 {
    (u64::from(t.dwHighDateTime) << 32) | u64::from(t.dwLowDateTime)
}
fn times(h: &Handle, pid: u32) -> Result<(Identity, u64)> {
    let mut created = FILETIME::default();
    let mut exit = created;
    let mut kernel = created;
    let mut user = created;
    // SAFETY: process handle is live and all outputs are writable FILETIME values.
    if unsafe { GetProcessTimes(h.0, &mut created, &mut exit, &mut kernel, &mut user) } == 0 {
        return Err(io("read process identity", std::io::Error::last_os_error()));
    }
    Ok((
        Identity {
            pid,
            started: ticks(created),
            started_sub: 0,
        },
        ticks(kernel)
            .saturating_add(ticks(user))
            .saturating_mul(100),
    ))
}
fn identity(pid: u32) -> Result<Identity> {
    times(&open(pid, PROCESS_QUERY_LIMITED_INFORMATION)?, pid).map(|v| v.0)
}
fn process(pid: u32) -> Result<Process> {
    let h = open(pid, PROCESS_QUERY_LIMITED_INFORMATION)?;
    let (identity, _) = times(&h, pid)?;
    let mut buf = vec![0u16; 32768];
    let mut len = buf.len() as u32;
    // SAFETY: buffer capacity matches len; h remains open.
    let executable =
        if unsafe { QueryFullProcessImageNameW(h.0, 0, buf.as_mut_ptr(), &mut len) } != 0 {
            path(&buf[..len as usize])
        } else {
            PathBuf::new()
        };
    let name = executable
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();
    Ok(Process {
        identity,
        name,
        executable,
        user: "unknown".into(),
        ..Process::default()
    })
}
fn username(pid: u32) -> String {
    let Ok(h) = open(pid, PROCESS_QUERY_LIMITED_INFORMATION) else {
        return "unknown".into();
    };
    let mut token = null_mut();
    // SAFETY: output is a writable handle slot and process handle is valid.
    if unsafe { OpenProcessToken(h.0, TOKEN_QUERY, &mut token) } == 0 {
        return "unknown".into();
    }
    let Ok(token) = Handle::new(token, "open process token") else {
        return "unknown".into();
    };
    let mut needed = 0;
    // SAFETY: null buffer with zero length queries required size.
    unsafe { GetTokenInformation(token.0, TokenUser, null_mut(), 0, &mut needed) };
    if needed == 0 || needed > 1024 * 1024 {
        return "unknown".into();
    }
    let mut data = vec![0usize; (needed as usize).div_ceil(size_of::<usize>())];
    // SAFETY: word-aligned allocation holds at least needed writable bytes.
    if unsafe {
        GetTokenInformation(
            token.0,
            TokenUser,
            data.as_mut_ptr().cast(),
            needed,
            &mut needed,
        )
    } == 0
    {
        return "unknown".into();
    }
    // SAFETY: successful TokenUser query initialized a TOKEN_USER at this aligned address.
    let sid = unsafe { (*(data.as_ptr().cast::<TOKEN_USER>())).User.Sid };
    let mut name = vec![0u16; 256];
    let mut domain = vec![0u16; 256];
    let mut n = name.len() as u32;
    let mut d = domain.len() as u32;
    let mut kind = 0;
    // SAFETY: SID belongs to live data buffer; names have lengths specified by n/d.
    if unsafe {
        LookupAccountSidW(
            null(),
            sid,
            name.as_mut_ptr(),
            &mut n,
            domain.as_mut_ptr(),
            &mut d,
            &mut kind,
        )
    } == 0
    {
        return "unknown".into();
    }
    let name = String::from_utf16_lossy(&name[..n as usize]);
    let domain = String::from_utf16_lossy(&domain[..d as usize]);
    if domain.is_empty() {
        name
    } else {
        format!("{domain}\\{name}")
    }
}
fn file_id(path: &Path) -> Option<(u32, u64)> {
    let name = wide(path).ok()?;
    // SAFETY: terminated UTF-16 path; metadata-only open, maximal sharing; no mutation.
    let h = Handle::new(
        unsafe {
            CreateFileW(
                name.as_ptr(),
                0,
                FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
                null(),
                OPEN_EXISTING,
                FILE_FLAG_BACKUP_SEMANTICS,
                null_mut(),
            )
        },
        "inspect file identity",
    )
    .ok()?;
    let mut info = BY_HANDLE_FILE_INFORMATION::default();
    // SAFETY: live file handle and writable correctly sized output.
    if unsafe { GetFileInformationByHandle(h.0, &mut info) } == 0 {
        return None;
    }
    Some((
        info.dwVolumeSerialNumber,
        (u64::from(info.nFileIndexHigh) << 32) | u64::from(info.nFileIndexLow),
    ))
}
fn matches(target: &Target, target_id: Option<(u32, u64)>, path: &Path) -> bool {
    if target.directory {
        return target.contains(path);
    }
    if let (Some(a), Some(b)) = (target_id, file_id(path)) {
        a == b
    } else {
        target.matches(path, None)
    }
}
struct Session(u32);
impl Drop for Session {
    fn drop(&mut self) {
        // SAFETY: session was acquired by RmStartSession and is ended exactly once.
        unsafe { RmEndSession(self.0) };
    }
}
fn rm_users(paths: &[PathBuf]) -> Result<Vec<RM_PROCESS_INFO>> {
    if paths.is_empty() {
        return Ok(Vec::new());
    }
    let mut session = 0;
    let mut key = [0u16; 33];
    // SAFETY: outputs match documented Restart Manager session and key sizes.
    let code = unsafe { RmStartSession(&mut session, 0, key.as_mut_ptr()) };
    if code != 0 {
        return Err(io(
            "start Restart Manager",
            std::io::Error::from_raw_os_error(code as i32),
        ));
    }
    let session = Session(session);
    let names: Vec<_> = paths.iter().map(|p| wide(p)).collect::<Result<_>>()?;
    let pointers: Vec<_> = names.iter().map(|n| n.as_ptr()).collect();
    // SAFETY: path pointers refer to terminated strings kept alive through this synchronous call.
    let code = unsafe {
        RmRegisterResources(
            session.0,
            pointers.len() as u32,
            pointers.as_ptr(),
            0,
            null(),
            0,
            null(),
        )
    };
    if code != 0 {
        return Err(io(
            "register resources",
            std::io::Error::from_raw_os_error(code as i32),
        ));
    }
    let mut records = Vec::<RM_PROCESS_INFO>::new();
    for _ in 0..5 {
        let mut needed = 0;
        let mut count = records.len() as u32;
        let mut reboot = 0;
        // SAFETY: empty list uses null; otherwise buffer has count writable initialized records.
        let code = unsafe {
            RmGetList(
                session.0,
                &mut needed,
                &mut count,
                if records.is_empty() {
                    null_mut()
                } else {
                    records.as_mut_ptr()
                },
                &mut reboot,
            )
        };
        if code == 0 {
            if count as usize > records.len() {
                return Err(Error::Unavailable(
                    "invalid Restart Manager result length".into(),
                ));
            }
            records.truncate(count as usize);
            return Ok(records);
        }
        if code != ERROR_MORE_DATA {
            return Err(io(
                "query resource users",
                std::io::Error::from_raw_os_error(code as i32),
            ));
        }
        if needed > 1_000_000 {
            return Err(Error::Unavailable(
                "Restart Manager result too large".into(),
            ));
        }
        records.resize_with(needed as usize + 16, RM_PROCESS_INFO::default);
    }
    Err(Error::Unavailable(
        "resources changed too quickly; refresh".into(),
    ))
}
fn sharing(path: &Path) -> Option<LockEvidence> {
    let name = wide(path).ok()?;
    for (access, kind) in [
        (GENERIC_READ, AccessKind::Read),
        (GENERIC_WRITE, AccessKind::Write),
        (DELETE, AccessKind::Delete),
    ] {
        // SAFETY: existing file only, maximal sharing; probes neither write data nor acquire byte-range locks.
        let h = unsafe {
            CreateFileW(
                name.as_ptr(),
                access,
                FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
                null(),
                OPEN_EXISTING,
                FILE_ATTRIBUTE_NORMAL,
                null_mut(),
            )
        };
        if h == INVALID_HANDLE_VALUE {
            // SAFETY: GetLastError has no preconditions and immediately follows failing call.
            if unsafe { GetLastError() } == ERROR_SHARING_VIOLATION {
                return Some(LockEvidence::SharingConflict(kind));
            }
        } else if !h.is_null() {
            drop(Handle(h))
        }
    }
    None
}
#[derive(Default)]
pub struct Native {
    sampler: Sampler,
}
fn process_snapshot() -> Result<(Handle, PROCESSENTRY32W)> {
    // SAFETY: valid snapshot flags; API returns an owned handle.
    let h = Handle::new(
        unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) },
        "snapshot processes",
    )?;
    let entry = PROCESSENTRY32W {
        dwSize: size_of::<PROCESSENTRY32W>() as u32,
        ..Default::default()
    };
    Ok((h, entry))
}
impl Backend for Native {
    fn scan(&mut self, target: &Target, cancel: &Cancellation) -> Result<Snapshot> {
        cancel.check()?;
        let mut snapshot=Snapshot{warnings:vec!["Windows: CWD, directory handles and deleted files are not visible. Sharing conflicts are per file; reported users are not proven lock owners. Byte-range locks are not enumerated.".into()],..Snapshot::default()};
        let mut limited = 0;
        let mut parents = HashMap::new();
        let target_id = if target.directory {
            None
        } else {
            file_id(&target.path)
        };
        let (h, mut entry) = process_snapshot()?;
        // SAFETY: initialized size field and live snapshot handle.
        let mut ok = unsafe { Process32FirstW(h.0, &mut entry) };
        while ok != 0 {
            cancel.check()?;
            let pid = entry.th32ProcessID;
            parents.insert(pid, entry.th32ParentProcessID);
            if pid > 0 && pid != std::process::id() {
                if let Ok(mut p) = process(pid) {
                    if !p.executable.as_os_str().is_empty()
                        && matches(target, target_id, &p.executable)
                    {
                        p.usages.push(Usage {
                            path: p.executable.clone(),
                            relation: Relation::Executable,
                            access: Access::Execute,
                            ..Usage::default()
                        })
                    }
                    let mut modules = Err(Error::Unavailable("module snapshot unavailable".into()));
                    for _ in 0..3 {
                        // SAFETY: documented Toolhelp flags, observed positive PID.
                        modules = Handle::new(
                            unsafe {
                                CreateToolhelp32Snapshot(
                                    TH32CS_SNAPMODULE | TH32CS_SNAPMODULE32,
                                    pid,
                                )
                            },
                            "snapshot modules",
                        );
                        if !matches!(&modules,Err(Error::Io{source,..}) if source.raw_os_error()==Some(ERROR_BAD_LENGTH as i32))
                        {
                            break;
                        }
                    }
                    if let Ok(modules) = modules {
                        let mut module = MODULEENTRY32W {
                            dwSize: size_of::<MODULEENTRY32W>() as u32,
                            ..Default::default()
                        };
                        // SAFETY: live snapshot and initialized output size.
                        let mut next = unsafe { Module32FirstW(modules.0, &mut module) };
                        while next != 0 {
                            cancel.check()?;
                            let path = path(&module.szExePath);
                            if matches(target, target_id, &path) {
                                p.usages.push(Usage {
                                    path,
                                    relation: Relation::Mapped,
                                    access: Access::Execute,
                                    ..Usage::default()
                                })
                            }
                            // SAFETY: same valid snapshot/output as above.
                            next = unsafe { Module32NextW(modules.0, &mut module) };
                        }
                    } else {
                        limited += 1
                    }
                    if !p.usages.is_empty() && identity(pid).is_ok_and(|id| id == p.identity) {
                        snapshot.processes.push(p)
                    }
                } else {
                    limited += 1
                }
            }
            // SAFETY: same live process snapshot and output.
            ok = unsafe { Process32NextW(h.0, &mut entry) };
        }
        // SAFETY: capture failure code immediately after enumeration ends.
        let code = unsafe { GetLastError() };
        if code != ERROR_NO_MORE_FILES {
            return Err(io(
                "enumerate processes",
                std::io::Error::from_raw_os_error(code as i32),
            ));
        }
        let mut cache = HashMap::new();
        let mut batch = Vec::with_capacity(128);
        let mut count = 0;
        let mut stack = vec![target.path.clone()];
        while let Some(path) = stack.pop() {
            cancel.check()?;
            if target.directory {
                let entries = match std::fs::read_dir(&path) {
                    Ok(e) => e,
                    Err(_) => {
                        limited += 1;
                        continue;
                    }
                };
                for entry in entries {
                    cancel.check()?;
                    let Ok(entry) = entry else {
                        limited += 1;
                        continue;
                    };
                    let Ok(kind) = entry.file_type() else {
                        limited += 1;
                        continue;
                    };
                    if kind.is_dir() {
                        stack.push(entry.path())
                    } else if kind.is_file() {
                        batch.push(entry.path());
                        count += 1;
                        if batch.len() == 128 {
                            correlate(&batch, &mut snapshot, &mut cache, &mut limited, cancel)?;
                            batch.clear()
                        }
                        if count >= 10_000 {
                            break;
                        }
                    }
                }
            } else {
                batch.push(path);
                count += 1
            }
            if count >= 10_000 {
                snapshot.warnings.push("Directory scan limited to 10,000 files. Narrow the target for complete coverage.".into());
                break;
            }
        }
        if !batch.is_empty() {
            correlate(&batch, &mut snapshot, &mut cache, &mut limited, cancel)?
        }
        snapshot.normalize();
        for p in &mut snapshot.processes {
            cancel.check()?;
            p.user = username(p.identity.pid);
            p.parent = parents.get(&p.identity.pid).copied().unwrap_or(0);
            let mut next = p.parent;
            while next > 0
                && next != p.identity.pid
                && p.ancestors.len() < 8
                && !p.ancestors.iter().any(|a| a.identity.pid == next)
            {
                cancel.check()?;
                match process(next) {
                    Ok(a) => {
                        p.ancestors.push(Ancestor {
                            identity: a.identity,
                            name: a.name,
                        });
                        next = parents.get(&next).copied().unwrap_or(0)
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
        }
        if limited > 0 {
            snapshot.warnings.push(format!(
                "{limited} process/resource inspections were unavailable (permissions or changes)."
            ))
        }
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
            let Ok(h) = open(id.pid, PROCESS_QUERY_INFORMATION | PROCESS_VM_READ) else {
                continue;
            };
            let Ok((current, cpu)) = times(&h, id.pid) else {
                continue;
            };
            if current != id {
                continue;
            }
            let mut m = PROCESS_MEMORY_COUNTERS {
                cb: size_of::<PROCESS_MEMORY_COUNTERS>() as u32,
                ..Default::default()
            };
            // SAFETY: live process handle, writable PROCESS_MEMORY_COUNTERS with its exact size.
            let memory = if unsafe {
                GetProcessMemoryInfo(h.0, &mut m, size_of::<PROCESS_MEMORY_COUNTERS>() as u32)
            } != 0
            {
                Some(m.WorkingSetSize as u64)
            } else {
                None
            };
            raw.push((id, cpu, memory));
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
        let h = open(
            id.pid,
            PROCESS_QUERY_LIMITED_INFORMATION
                | 0x00100000
                | if force { PROCESS_TERMINATE } else { 0 },
        )?;
        if times(&h, id.pid)?.0 != id {
            return Err(Error::Changed);
        }
        if force {
            // SAFETY: owned process handle pins the exact validated lifetime and has terminate access.
            if unsafe { TerminateProcess(h.0, 1) } == 0 {
                return Err(io("terminate process", std::io::Error::last_os_error()));
            }
            return Ok(());
        }
        let mut state = CloseState {
            pid: id.pid,
            sent: 0,
            handle: h.0,
        };
        // SAFETY: callback receives a pointer to state, valid for synchronous EnumWindows. h remains open.
        if unsafe { EnumWindows(Some(close_window), (&mut state as *mut CloseState) as isize) } == 0
        {
            return Err(io("enumerate windows", std::io::Error::last_os_error()));
        }
        if state.sent == 0 {
            return Err(Error::Unavailable(
                "no window accepted a graceful close request; use force kill explicitly if needed"
                    .into(),
            ));
        }
        Ok(())
    }
}
struct CloseState {
    pid: u32,
    sent: usize,
    handle: HANDLE,
}
unsafe extern "system" fn close_window(hwnd: HWND, param: isize) -> i32 {
    // SAFETY: only called synchronously by our EnumWindows with the live CloseState address.
    let state = unsafe { &mut *(param as *mut CloseState) };
    let mut pid = 0;
    // SAFETY: OS supplies hwnd and writable PID slot. A still-running pinned process cannot have its PID reused.
    unsafe {
        GetWindowThreadProcessId(hwnd, &mut pid);
        if pid == state.pid
            && WaitForSingleObject(state.handle, 0) == WAIT_TIMEOUT
            && PostMessageW(hwnd, WM_CLOSE, 0, 0) != 0
        {
            state.sent += 1
        }
    }
    1
}
fn correlate(
    paths: &[PathBuf],
    snapshot: &mut Snapshot,
    cache: &mut HashMap<Identity, Process>,
    limited: &mut usize,
    cancel: &Cancellation,
) -> Result<()> {
    cancel.check()?;
    let apps = match rm_users(paths) {
        Ok(a) => a,
        Err(_) => {
            *limited += 1;
            return Ok(());
        }
    };
    if apps.is_empty() {
        return Ok(());
    }
    if paths.len() > 1 {
        let mid = paths.len() / 2;
        correlate(&paths[..mid], snapshot, cache, limited, cancel)?;
        return correlate(&paths[mid..], snapshot, cache, limited, cancel);
    }
    let lock = sharing(&paths[0]);
    for app in apps {
        let id = Identity {
            pid: app.Process.dwProcessId,
            started: ticks(app.Process.ProcessStartTime),
            started_sub: 0,
        };
        if id.pid == std::process::id() {
            continue;
        }
        let mut p = if let Some(p) = cache.get(&id) {
            if !identity(id.pid).is_ok_and(|i| i == id) {
                continue;
            }
            p.clone()
        } else {
            match process(id.pid) {
                Ok(p) if p.identity == id => {
                    cache.insert(id, p.clone());
                    p
                }
                Ok(_) => continue,
                Err(_) => {
                    *limited += 1;
                    Process {
                        identity: id,
                        name: path(&app.strAppName).to_string_lossy().into_owned(),
                        user: "unknown".into(),
                        ..Process::default()
                    }
                }
            }
        };
        p.usages.push(Usage {
            path: paths[0].clone(),
            relation: Relation::RestartManager,
            ..Usage::default()
        });
        if let Some(lock) = &lock {
            p.usages.push(Usage {
                path: paths[0].clone(),
                relation: Relation::Locked,
                lock: Some(lock.clone()),
                ..Usage::default()
            })
        }
        snapshot.processes.push(p);
    }
    Ok(())
}
