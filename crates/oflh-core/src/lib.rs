//! Platform-independent process observations, path matching, and search semantics.
#![forbid(unsafe_code)]
#![deny(missing_docs)]
#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used))]

mod path;
pub mod search;

pub use path::Target;
use std::{
    collections::{BTreeMap, btree_map::Entry},
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};
use thiserror::Error;

/// Recoverable domain failures shared by the UI and native backends.
#[derive(Debug, Error)]
pub enum Error {
    /// An OS call failed; its original error remains available through `source()`.
    #[error("{operation}: {source}")]
    Io {
        /// Short description of the operation that failed.
        operation: &'static str,
        /// Original operating-system error, including its native error code.
        #[source]
        source: std::io::Error,
    },
    /// A previously captured process lifetime no longer matches its PID.
    #[error("process exited or PID was reused; refresh before trying again")]
    Changed,
    /// The target is protected, is this program, or lacks a usable birth token.
    #[error("refusing to terminate a protected process or an unavailable identity")]
    Protected,
    /// A newer operation or shutdown cancelled this operation cooperatively.
    #[error("operation cancelled")]
    Cancelled,
    /// A platform capability is unavailable or returned an invalid observation.
    #[error("{0}")]
    Unavailable(String),
}

/// A domain operation that may fail without panicking.
pub type Result<T> = std::result::Result<T, Error>;

/// Attach operation context without discarding the native error code.
pub fn io(operation: &'static str, source: std::io::Error) -> Error {
    Error::Io { operation, source }
}

/// Cheap, clonable cooperative cancellation shared with one background operation.
#[derive(Clone, Default)]
pub struct Cancellation(Arc<AtomicBool>);

impl Cancellation {
    /// Request cancellation. Native calls already in progress may finish first.
    pub fn cancel(&self) {
        // The flag publishes no associated data, so relaxed ordering is sufficient.
        self.0.store(true, Ordering::Relaxed);
    }

    /// Return a typed cancellation error at an operation boundary.
    pub fn check(&self) -> Result<()> {
        if self.0.load(Ordering::Relaxed) {
            Err(Error::Cancelled)
        } else {
            Ok(())
        }
    }
}

/// A process lifetime, rather than a reusable PID alone.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct Identity {
    /// Native process identifier.
    pub pid: u32,
    /// Linux start ticks, Windows creation FILETIME, or macOS birth seconds.
    pub started: u64,
    /// macOS birth microseconds; zero on platforms with a single birth counter.
    pub started_sub: u64,
}

impl Identity {
    /// Reject intrinsically unsafe action targets before accessing native APIs.
    ///
    /// Backends must also compare this token against a fresh native observation.
    pub fn validate(self) -> Result<()> {
        if self.pid <= 1 || self.pid == std::process::id() || self.started == 0 {
            Err(Error::Protected)
        } else {
            Ok(())
        }
    }
}

/// Observed access permission; this does not imply live I/O or a file lock.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub enum Access {
    /// The platform did not expose an access mode.
    #[default]
    Unknown,
    /// Read access was observed.
    Read,
    /// Write access was observed.
    Write,
    /// Read and write access were observed.
    ReadWrite,
    /// Executable image or executable memory mapping.
    Execute,
    /// Directory reference, including a working directory.
    Directory,
    /// Reference-only descriptor, such as Linux `O_PATH`.
    Reference,
    /// A mapping whose access flags are unknown.
    Mapped,
}

impl Access {
    /// Stable text used in the UI and search index.
    pub fn label(self) -> &'static str {
        match self {
            Self::Unknown => "unknown",
            Self::Read => "read",
            Self::Write => "write",
            Self::ReadWrite => "read/write",
            Self::Execute => "execute",
            Self::Directory => "directory",
            Self::Reference => "reference",
            Self::Mapped => "mapped",
        }
    }
}

/// How a process was observed to reference a file.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub enum Relation {
    /// An open file descriptor.
    #[default]
    Open,
    /// The process working directory.
    Cwd,
    /// The executable image.
    Executable,
    /// A mapped file or loaded module.
    Mapped,
    /// Additional native lock or sharing-conflict evidence exists.
    Locked,
    /// Windows Restart Manager identified a resource user, not a proven lock owner.
    RestartManager,
}

impl Relation {
    /// Stable text used in the UI and search index.
    pub fn label(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Cwd => "cwd",
            Self::Executable => "executable",
            Self::Mapped => "mapped",
            Self::Locked => "locked",
            Self::RestartManager => "restart manager",
        }
    }
}

/// Evidence stronger than an ordinary open-file observation.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub enum LockEvidence {
    /// Kernel-reported lock type, mode, and byte range.
    Kernel(String),
    /// A Windows sharing restriction; the reported user's ownership is unverified.
    SharingConflict(AccessKind),
}

/// Requested access rejected by a Windows sharing compatibility probe.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub enum AccessKind {
    /// Read access was denied by sharing flags.
    Read,
    /// Write access was denied by sharing flags.
    Write,
    /// Delete access was denied by sharing flags.
    Delete,
}

impl std::fmt::Display for LockEvidence {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Kernel(description) => formatter.write_str(description),
            Self::SharingConflict(kind) => {
                let access = match kind {
                    AccessKind::Read => "read",
                    AccessKind::Write => "write",
                    AccessKind::Delete => "delete",
                };
                write!(
                    formatter,
                    "SHARING CONFLICT: {access} denied; reported file user, lock owner unverified"
                )
            }
        }
    }
}

/// One distinct observation; different relations and evidence remain separate.
#[derive(Clone, Debug, Default, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct Usage {
    /// Native path, retained losslessly for filesystem operations.
    pub path: PathBuf,
    /// Source of the process-to-file association.
    pub relation: Relation,
    /// Observed access mode.
    pub access: Access,
    /// Whether the referenced file has been unlinked while remaining open.
    pub deleted: bool,
    /// Additional evidence; `None` means no confirmed lock was observed.
    pub lock: Option<LockEvidence>,
}

/// A captured ancestor whose identity must survive UI refreshes unchanged.
#[derive(Clone, Debug, Default)]
pub struct Ancestor {
    /// Captured process lifetime; a zero birth token is not actionable.
    pub identity: Identity,
    /// Untrusted OS name, sanitized only when rendered.
    pub name: String,
}

/// A process and all its observations matching the scan target.
#[derive(Clone, Debug, Default)]
pub struct Process {
    /// Captured process lifetime.
    pub identity: Identity,
    /// OS-supplied process name.
    pub name: String,
    /// Resolved account name, numeric ID, or `unknown`.
    pub user: String,
    /// Native executable path, when available.
    pub executable: PathBuf,
    /// Native working directory, when available.
    pub cwd: PathBuf,
    /// Distinct observations matching the scan target.
    pub usages: Vec<Usage>,
    /// Observed immediate parent PID.
    pub parent: u32,
    /// Up to eight ancestors, nearest parent first.
    pub ancestors: Vec<Ancestor>,
    /// Resident bytes; `None` is unavailable, not zero.
    pub memory: Option<u64>,
    /// Percentage of total machine CPU capacity between two samples.
    pub cpu: Option<f64>,
}

/// A complete scan result published atomically to the UI.
#[derive(Clone, Debug, Default)]
pub struct Snapshot {
    /// Matching processes, ordered by PID after normalization.
    pub processes: Vec<Process>,
    /// Explicit coverage limitations and partial-inspection notices.
    pub warnings: Vec<String>,
}

impl Snapshot {
    /// Merge duplicate process observations without discarding distinct evidence.
    pub fn normalize(&mut self) {
        let mut groups = BTreeMap::<Identity, Process>::new();
        for process in self.processes.drain(..) {
            match groups.entry(process.identity) {
                Entry::Vacant(entry) => {
                    entry.insert(process);
                }
                Entry::Occupied(mut entry) => {
                    entry.get_mut().usages.extend(process.usages);
                }
            }
        }
        self.processes = groups.into_values().collect();
        for process in &mut self.processes {
            process.usages.sort_unstable();
            process.usages.dedup();
        }
    }
}

/// A lightweight resource update that does not repeat file discovery.
#[derive(Clone, Copy, Debug, Default)]
pub struct Metrics {
    /// Resident bytes, if inspection succeeded.
    pub memory: Option<u64>,
    /// Total-machine CPU percentage, if two valid samples exist.
    pub cpu: Option<f64>,
}

/// Replace terminal controls and formatting controls in untrusted display text.
///
/// The original native path must still be used for filesystem operations.
pub fn safe(value: &str) -> String {
    use unicode_general_category::{GeneralCategory, get_general_category};
    value
        .chars()
        .map(|character| {
            if character.is_control() || get_general_category(character) == GeneralCategory::Format
            {
                '�'
            } else {
                character
            }
        })
        .collect()
}
