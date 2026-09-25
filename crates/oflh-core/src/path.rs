use crate::{Result, io};
use std::{
    fs,
    path::{Component, Path, PathBuf},
};
/// Canonical scan target and its identity at the start of an investigation.
#[derive(Clone, Debug)]
pub struct Target {
    /// Absolute path with the longest existing ancestor resolved.
    pub path: PathBuf,
    /// Whether the target existed as a directory when resolved.
    pub directory: bool,
    /// Initial metadata used to distinguish aliases from file replacements.
    pub metadata: Option<fs::Metadata>,
}
impl Target {
    /// Resolve a user path, including a missing leaf below an existing ancestor.
    pub fn new(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let path = if path.is_absolute() {
            path.to_owned()
        } else {
            std::env::current_dir()
                .map_err(|error| io("current directory", error))?
                .join(path)
        };
        let path = canonical(&path).map_err(|error| io("resolve target", error))?;
        let metadata = match fs::metadata(&path) {
            Ok(metadata) => Some(metadata),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => return Err(io("inspect target", error)),
        };
        Ok(Self {
            directory: metadata.as_ref().is_some_and(|metadata| metadata.is_dir()),
            path,
            metadata,
        })
    }
    /// Test lexical containment of an already resolved native observation path.
    pub fn contains(&self, path: &Path) -> bool {
        contains(&self.path, path)
    }
    /// Match an observation using file identity when available, otherwise its path.
    pub fn matches(&self, path: &Path, meta: Option<&fs::Metadata>) -> bool {
        if self.directory {
            return self.contains(path);
        }
        #[cfg(unix)]
        if let (Some(target_metadata), Some(candidate_metadata)) = (&self.metadata, meta) {
            use std::os::unix::fs::MetadataExt;
            return target_metadata.dev() == candidate_metadata.dev()
                && target_metadata.ino() == candidate_metadata.ino();
        }
        #[cfg(not(unix))]
        let _ = meta; // Windows file IDs are checked by the native backend before this fallback.
        normalize(&self.path) == normalize(path)
    }
}
fn canonical(path: &Path) -> std::io::Result<PathBuf> {
    match fs::canonicalize(path) {
        Ok(resolved) => Ok(resolved),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            match (path.parent(), path.file_name()) {
                (Some(parent), Some(name)) => Ok(canonical(parent)?.join(name)),
                _ => Ok(clean(path)),
            }
        }
        Err(error) => Err(error),
    }
}
fn clean(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            _ => out.push(component),
        }
    }
    out
}
fn normalize(path: &Path) -> PathBuf {
    #[cfg(windows)]
    {
        use std::ffi::OsString;
        use std::os::windows::ffi::{OsStrExt, OsStringExt};

        // Windows paths can contain unpaired UTF-16 surrogates. Preserve them
        // during case folding so distinct native names never alias through U+FFFD.
        let cleaned = clean(path);
        let mut normalized = Vec::new();
        let mut encoded = [0u16; 2];
        for character in char::decode_utf16(cleaned.as_os_str().encode_wide()) {
            match character {
                Ok('/') => normalized.push(u16::from(b'\\')),
                Ok(character) => {
                    for lowercase in character.to_lowercase() {
                        normalized.extend_from_slice(lowercase.encode_utf16(&mut encoded));
                    }
                }
                Err(error) => normalized.push(error.unpaired_surrogate()),
            }
        }
        let verbatim_prefix: Vec<_> = "\\\\?\\".encode_utf16().collect();
        let unc_prefix: Vec<_> = "\\\\?\\unc\\".encode_utf16().collect();
        if let Some(suffix) = normalized.strip_prefix(unc_prefix.as_slice()) {
            let mut network_path = vec![u16::from(b'\\'); 2];
            network_path.extend_from_slice(suffix);
            return PathBuf::from(OsString::from_wide(&network_path));
        }
        PathBuf::from(OsString::from_wide(
            normalized
                .strip_prefix(verbatim_prefix.as_slice())
                .unwrap_or(&normalized),
        ))
    }
    #[cfg(not(windows))]
    {
        clean(path)
    }
}
pub fn contains(parent: &Path, child: &Path) -> bool {
    normalize(child).starts_with(normalize(parent))
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use std::ffi::OsString;
    use std::os::windows::ffi::OsStringExt;

    #[test]
    fn comparison_preserves_distinct_unpaired_surrogates() {
        let prefix: Vec<_> = "C:\\files\\".encode_utf16().collect();
        let mut first = prefix.clone();
        first.push(0xd800);
        let mut second = prefix;
        second.push(0xd801);
        let first = PathBuf::from(OsString::from_wide(&first));
        let second = PathBuf::from(OsString::from_wide(&second));
        assert_ne!(normalize(&first), normalize(&second));
        assert!(!contains(&first, &second));
        assert!(contains(&first, &first.join("child")));
    }

    #[test]
    fn windows_prefixes_and_case_are_equivalent() {
        assert_eq!(
            normalize(Path::new("C:\\BUILD\\File.dll")),
            normalize(Path::new("\\\\?\\C:\\build\\file.dll"))
        );
        assert_eq!(
            normalize(Path::new("\\\\server\\share\\File.dll")),
            normalize(Path::new("\\\\?\\UNC\\SERVER\\share\\file.dll"))
        );
    }
}
