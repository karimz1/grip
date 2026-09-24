//! Platform-independent observations and query semantics.
#![forbid(unsafe_code)]
mod path;
pub mod search;
pub use path::Target;
use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("{operation}: {source}")]
    Io {
        operation: &'static str,
        #[source]
        source: std::io::Error,
    },
    #[error("process exited or PID was reused; refresh before trying again")]
    Changed,
    #[error("refusing to terminate a protected process or an unavailable identity")]
    Protected,
    #[error("operation cancelled")]
    Cancelled,
    #[error("{0}")]
    Unavailable(String),
}
pub type Result<T> = std::result::Result<T, Error>;
pub fn io(operation: &'static str, source: std::io::Error) -> Error {
    Error::Io { operation, source }
}

#[derive(Clone, Default)]
pub struct Cancellation(Arc<AtomicBool>);
impl Cancellation {
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Relaxed);
    }
    pub fn check(&self) -> Result<()> {
        if self.0.load(Ordering::Relaxed) {
            Err(Error::Cancelled)
        } else {
            Ok(())
        }
    }
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct Identity {
    pub pid: u32,
    pub started: u64,
    pub started_sub: u64,
}
impl Identity {
    pub fn validate(self) -> Result<()> {
        if self.pid <= 1 || self.pid == std::process::id() || self.started == 0 {
            Err(Error::Protected)
        } else {
            Ok(())
        }
    }
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub enum Access {
    #[default]
    Unknown,
    Read,
    Write,
    ReadWrite,
    Execute,
    Directory,
    Reference,
    Mapped,
}
impl Access {
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
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub enum Relation {
    #[default]
    Open,
    Cwd,
    Executable,
    Mapped,
    Locked,
    RestartManager,
}
impl Relation {
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
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub enum LockEvidence {
    Kernel(String),
    SharingConflict(AccessKind),
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub enum AccessKind {
    Read,
    Write,
    Delete,
}
impl std::fmt::Display for LockEvidence {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Kernel(s) => f.write_str(s),
            Self::SharingConflict(kind) => write!(
                f,
                "SHARING CONFLICT: {} denied; reported file user, lock owner unverified",
                match kind {
                    AccessKind::Read => "read",
                    AccessKind::Write => "write",
                    AccessKind::Delete => "delete",
                }
            ),
        }
    }
}
#[derive(Clone, Debug, Default, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct Usage {
    pub path: PathBuf,
    pub relation: Relation,
    pub access: Access,
    pub deleted: bool,
    pub lock: Option<LockEvidence>,
}
#[derive(Clone, Debug, Default)]
pub struct Ancestor {
    pub identity: Identity,
    pub name: String,
}
#[derive(Clone, Debug, Default)]
pub struct Process {
    pub identity: Identity,
    pub name: String,
    pub user: String,
    pub executable: PathBuf,
    pub cwd: PathBuf,
    pub usages: Vec<Usage>,
    pub parent: u32,
    pub ancestors: Vec<Ancestor>,
    pub memory: Option<u64>,
    pub cpu: Option<f64>,
}
#[derive(Clone, Debug, Default)]
pub struct Snapshot {
    pub processes: Vec<Process>,
    pub warnings: Vec<String>,
}
impl Snapshot {
    pub fn normalize(&mut self) {
        let mut groups = BTreeMap::<Identity, Process>::new();
        for p in self.processes.drain(..) {
            match groups.entry(p.identity) {
                std::collections::btree_map::Entry::Vacant(e) => {
                    e.insert(p);
                }
                std::collections::btree_map::Entry::Occupied(mut e) => {
                    e.get_mut().usages.extend(p.usages)
                }
            }
        }
        self.processes = groups
            .into_values()
            .map(|mut p| {
                p.usages.sort_unstable();
                p.usages.dedup();
                p
            })
            .collect();
    }
}
#[derive(Clone, Copy, Debug, Default)]
pub struct Metrics {
    pub memory: Option<u64>,
    pub cpu: Option<f64>,
}
/// OS text must never be emitted as terminal control sequences.
pub fn safe(value: &str) -> String {
    use unicode_general_category::{GeneralCategory, get_general_category};
    value
        .chars()
        .map(|c| {
            if c.is_control() || get_general_category(c) == GeneralCategory::Format {
                '�'
            } else {
                c
            }
        })
        .collect()
}
