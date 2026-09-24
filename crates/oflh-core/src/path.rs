use crate::{Result, io};
use std::{
    fs,
    path::{Component, Path, PathBuf},
};
#[derive(Clone, Debug)]
pub struct Target {
    pub path: PathBuf,
    pub directory: bool,
    pub metadata: Option<fs::Metadata>,
}
impl Target {
    pub fn new(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let path = if path.is_absolute() {
            path.to_owned()
        } else {
            std::env::current_dir()
                .map_err(|e| io("current directory", e))?
                .join(path)
        };
        let path = canonical(&path).map_err(|e| io("resolve target", e))?;
        let metadata = match fs::metadata(&path) {
            Ok(m) => Some(m),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
            Err(e) => return Err(io("inspect target", e)),
        };
        Ok(Self {
            directory: metadata.as_ref().is_some_and(|m| m.is_dir()),
            path,
            metadata,
        })
    }
    pub fn contains(&self, path: &Path) -> bool {
        contains(&self.path, path)
    }
    pub fn matches(&self, path: &Path, meta: Option<&fs::Metadata>) -> bool {
        if self.directory {
            return self.contains(path);
        }
        #[cfg(unix)]
        if let (Some(a), Some(b)) = (&self.metadata, meta) {
            use std::os::unix::fs::MetadataExt;
            return a.dev() == b.dev() && a.ino() == b.ino();
        }
        #[cfg(not(unix))]
        let _ = meta; // Windows file IDs are checked by the native backend before this fallback.
        normalize(&self.path) == normalize(path)
    }
}
fn canonical(path: &Path) -> std::io::Result<PathBuf> {
    match fs::canonicalize(path) {
        Ok(p) => Ok(p),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            match (path.parent(), path.file_name()) {
                (Some(parent), Some(name)) => Ok(canonical(parent)?.join(name)),
                _ => Ok(clean(path)),
            }
        }
        Err(e) => Err(e),
    }
}
fn clean(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in path.components() {
        match c {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            _ => out.push(c),
        }
    }
    out
}
fn normalize(path: &Path) -> PathBuf {
    #[cfg(windows)]
    {
        let s = clean(path)
            .to_string_lossy()
            .replace('/', "\\")
            .to_lowercase();
        if let Some(s) = s.strip_prefix("\\\\?\\unc\\") {
            return PathBuf::from(format!("\\\\{s}"));
        }
        PathBuf::from(s.strip_prefix("\\\\?\\").unwrap_or(&s))
    }
    #[cfg(not(windows))]
    {
        clean(path)
    }
}
pub fn contains(parent: &Path, child: &Path) -> bool {
    normalize(child).starts_with(normalize(parent))
}
