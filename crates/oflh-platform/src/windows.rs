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
    fn new(handle: HANDLE, operation: &'static str) -> Result<Self> {
        if handle.is_null() || handle == INVALID_HANDLE_VALUE {
            Err(io(operation, std::io::Error::last_os_error()))
        } else {
            Ok(Self(handle))
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
fn wide(path: &Path) -> Result<Vec<u16>> {
    let mut encoded: Vec<_> = path.as_os_str().encode_wide().collect();
    if encoded.contains(&0) {
        return Err(Error::Unavailable("path contains NUL".into()));
    }
    encoded.push(0);
    Ok(encoded)
}
fn path(encoded: &[u16]) -> PathBuf {
    PathBuf::from(OsString::from_wide(
        &encoded[..encoded
            .iter()
            .position(|&c| c == 0)
            .unwrap_or(encoded.len())],
    ))
}
fn ticks(file_time: FILETIME) -> u64 {
    (u64::from(file_time.dwHighDateTime) << 32) | u64::from(file_time.dwLowDateTime)
}
fn times(handle: &Handle, pid: u32) -> Result<(Identity, u64)> {
    let mut created = FILETIME::default();
    let mut exit = created;
    let mut kernel = created;
    let mut user = created;
    // SAFETY: process handle is live and all outputs are writable FILETIME values.
    if unsafe { GetProcessTimes(handle.0, &mut created, &mut exit, &mut kernel, &mut user) } == 0 {
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
fn read_identity(pid: u32) -> Result<Identity> {
    times(&open(pid, PROCESS_QUERY_LIMITED_INFORMATION)?, pid).map(|v| v.0)
}
fn read_process(pid: u32) -> Result<Process> {
    let handle = open(pid, PROCESS_QUERY_LIMITED_INFORMATION)?;
    let (identity, _) = times(&handle, pid)?;
    let mut buf = vec![0u16; 32768];
    let mut len = buf.len() as u32;
    // SAFETY: buffer capacity matches len; h remains open.
    let executable =
        if unsafe { QueryFullProcessImageNameW(handle.0, 0, buf.as_mut_ptr(), &mut len) } != 0 {
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
    let Ok(handle) = open(pid, PROCESS_QUERY_LIMITED_INFORMATION) else {
        return "unknown".into();
    };
    let mut token = null_mut();
    // SAFETY: output is a writable handle slot and process handle is valid.
    if unsafe { OpenProcessToken(handle.0, TOKEN_QUERY, &mut token) } == 0 {
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
    let mut name_length = name.len() as u32;
    let mut domain_length = domain.len() as u32;
    let mut kind = 0;
    // SAFETY: SID belongs to live data buffer; names have lengths specified by n/d.
    if unsafe {
        LookupAccountSidW(
            null(),
            sid,
            name.as_mut_ptr(),
            &mut name_length,
            domain.as_mut_ptr(),
            &mut domain_length,
            &mut kind,
        )
    } == 0
    {
        return "unknown".into();
    }
    let name = String::from_utf16_lossy(&name[..name_length as usize]);
    let domain = String::from_utf16_lossy(&domain[..domain_length as usize]);
    if domain.is_empty() {
        name
    } else {
        format!("{domain}\\{name}")
    }
}
fn file_id(path: &Path) -> Option<(u32, u64)> {
    let name = wide(path).ok()?;
    // SAFETY: terminated UTF-16 path; metadata-only open, maximal sharing; no mutation.
    let handle = Handle::new(
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
    if unsafe { GetFileInformationByHandle(handle.0, &mut info) } == 0 {
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
    let names: Vec<_> = paths
        .iter()
        .map(|process| wide(process))
        .collect::<Result<_>>()?;
    let pointers: Vec<_> = names
        .iter()
        .map(|name_length| name_length.as_ptr())
        .collect();
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
        let handle = unsafe {
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
        if handle == INVALID_HANDLE_VALUE {
            // SAFETY: GetLastError has no preconditions and immediately follows failing call.
            if unsafe { GetLastError() } == ERROR_SHARING_VIOLATION {
                return Some(LockEvidence::SharingConflict(kind));
            }
        } else if !handle.is_null() {
            drop(Handle(handle))
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
    let handle = Handle::new(
        unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) },
        "snapshot processes",
    )?;
    let entry = PROCESSENTRY32W {
        dwSize: size_of::<PROCESSENTRY32W>() as u32,
        ..Default::default()
    };
    Ok((handle, entry))
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
        let (handle, mut entry) = process_snapshot()?;
        // SAFETY: initialized size field and live snapshot handle.
        let mut ok = unsafe { Process32FirstW(handle.0, &mut entry) };
        while ok != 0 {
            cancel.check()?;
            let pid = entry.th32ProcessID;
            parents.insert(pid, entry.th32ParentProcessID);
            if pid > 0 && pid != std::process::id() {
                if let Ok(mut process) = read_process(pid) {
                    if !process.executable.as_os_str().is_empty()
                        && matches(target, target_id, &process.executable)
                    {
                        process.usages.push(Usage {
                            path: process.executable.clone(),
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
                                process.usages.push(Usage {
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
                    if !process.usages.is_empty()
                        && read_identity(pid).is_ok_and(|identity| identity == process.identity)
                    {
                        snapshot.processes.push(process)
                    }
                } else {
                    limited += 1
                }
            }
            // SAFETY: same live process snapshot and output.
            ok = unsafe { Process32NextW(handle.0, &mut entry) };
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
                    Ok(error) => error,
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
        for process in &mut snapshot.processes {
            cancel.check()?;
            process.user = username(process.identity.pid);
            process.parent = parents.get(&process.identity.pid).copied().unwrap_or(0);
            let mut next = process.parent;
            while next > 0
                && next != process.identity.pid
                && process.ancestors.len() < 8
                && !process.ancestors.iter().any(|a| a.identity.pid == next)
            {
                cancel.check()?;
                match read_process(next) {
                    Ok(a) => {
                        process.ancestors.push(Ancestor {
                            identity: a.identity,
                            name: a.name,
                        });
                        next = parents.get(&next).copied().unwrap_or(0)
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
        }
        if limited > 0 {
            snapshot.warnings.push(format!(
                "{limited} process/resource inspections were unavailable (permissions or changes)."
            ))
        }
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
        let mut raw = Vec::with_capacity(ids.len());
        for &identity in ids {
            cancel.check()?;
            let Ok(handle) = open(identity.pid, PROCESS_QUERY_INFORMATION | PROCESS_VM_READ) else {
                continue;
            };
            let Ok((current, cpu)) = times(&handle, identity.pid) else {
                continue;
            };
            if current != identity {
                continue;
            }
            let mut memory_info = PROCESS_MEMORY_COUNTERS {
                cb: size_of::<PROCESS_MEMORY_COUNTERS>() as u32,
                ..Default::default()
            };
            // SAFETY: live process handle, writable PROCESS_MEMORY_COUNTERS with its exact size.
            let memory = if unsafe {
                GetProcessMemoryInfo(
                    handle.0,
                    &mut memory_info,
                    size_of::<PROCESS_MEMORY_COUNTERS>() as u32,
                )
            } != 0
            {
                Some(memory_info.WorkingSetSize as u64)
            } else {
                None
            };
            raw.push((identity, cpu, memory));
        }
        let total = self.sampler.clock().saturating_mul(
            std::thread::available_parallelism().map_or(1, |name_length| name_length.get()) as u64,
        );
        Ok(self.sampler.sample(raw, total))
    }
    fn terminate(&mut self, identity: Identity, force: bool, cancel: &Cancellation) -> Result<()> {
        identity.validate()?;
        cancel.check()?;
        let handle = open(
            identity.pid,
            PROCESS_QUERY_LIMITED_INFORMATION
                | 0x00100000
                | if force { PROCESS_TERMINATE } else { 0 },
        )?;
        if times(&handle, identity.pid)?.0 != identity {
            return Err(Error::Changed);
        }
        if force {
            // SAFETY: owned process handle pins the exact validated lifetime and has terminate access.
            if unsafe { TerminateProcess(handle.0, 1) } == 0 {
                return Err(io("terminate process", std::io::Error::last_os_error()));
            }
            return Ok(());
        }
        let mut state = CloseState {
            pid: identity.pid,
            sent: 0,
            handle: handle.0,
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
        let identity = Identity {
            pid: app.Process.dwProcessId,
            started: ticks(app.Process.ProcessStartTime),
            started_sub: 0,
        };
        if identity.pid == std::process::id() {
            continue;
        }
        let mut process = if let Some(process) = cache.get(&identity) {
            if !read_identity(identity.pid).is_ok_and(|i| i == identity) {
                continue;
            }
            process.clone()
        } else {
            match read_process(identity.pid) {
                Ok(process) if process.identity == identity => {
                    cache.insert(identity, process.clone());
                    process
                }
                Ok(_) => continue,
                Err(_) => {
                    *limited += 1;
                    Process {
                        identity,
                        name: path(&app.strAppName).to_string_lossy().into_owned(),
                        user: "unknown".into(),
                        ..Process::default()
                    }
                }
            }
        };
        process.usages.push(Usage {
            path: paths[0].clone(),
            relation: Relation::RestartManager,
            ..Usage::default()
        });
        if let Some(lock) = &lock {
            process.usages.push(Usage {
                path: paths[0].clone(),
                relation: Relation::Locked,
                lock: Some(lock.clone()),
                ..Usage::default()
            })
        }
        snapshot.processes.push(process);
    }
    Ok(())
}
