//! Developer/release executable. Never linked into the installed application.
#![forbid(unsafe_code)]
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs::{self, File},
    io::{Read, Write},
    path::{Path, PathBuf},
};
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
const TARGETS: [(&str, &str); 6] = [
    ("linux", "amd64"),
    ("linux", "arm64"),
    ("darwin", "amd64"),
    ("darwin", "arm64"),
    ("windows", "amd64"),
    ("windows", "arm64"),
];
fn version(s: &str) -> Result<()> {
    if s == "dev" {
        return Ok(());
    }
    let Some(v) = s.strip_prefix('v') else {
        return Err("version must be dev or v-prefixed semantic version".into());
    };
    semver::Version::parse(v)?;
    Ok(())
}
fn name(os: &str, arch: &str) -> Result<String> {
    if !TARGETS.contains(&(os, arch)) {
        return Err("unsupported target".into());
    }
    Ok(format!(
        "oflh-{os}-{arch}{}",
        if os == "windows" { ".exe" } else { "" }
    ))
}
fn digest(path: &Path) -> Result<String> {
    let mut input = File::open(path)?;
    let mut hash = Sha256::new();
    let mut buf = [0u8; 65536];
    loop {
        let n = input.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hash.update(&buf[..n]);
    }
    Ok(format!("{:x}", hash.finalize()))
}
fn package(binary: &Path, output: &Path, tag: &str, os: &str, arch: &str) -> Result<PathBuf> {
    version(tag)?;
    let name = name(os, arch)?;
    if !binary.is_file() || binary.metadata()?.len() == 0 {
        return Err("empty or missing executable".into());
    }
    fs::create_dir_all(output)?;
    let destination = output.join(name);
    fs::copy(binary, &destination)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&destination, fs::Permissions::from_mode(0o755))?;
    }
    Ok(destination)
}
fn formula(tag: &str, sums: &BTreeMap<String, String>, base: Option<&str>) -> Result<String> {
    version(tag)?;
    if tag == "dev" {
        return Err("Homebrew requires a tagged release".into());
    }
    let base = base.map(str::to_owned).unwrap_or_else(|| {
        format!("https://github.com/karimz1/open-file-lock-handle/releases/download/{tag}")
    });
    if base.contains(['"', '\n', '\r', '\\']) {
        return Err("invalid base URL".into());
    }
    let mut out = format!(
        "class Oflh < Formula\n  desc \"Find processes using files, directories, and open handles\"\n  homepage \"https://github.com/karimz1/open-file-lock-handle\"\n  version \"{}\"\n  license \"MIT\"\n\n",
        &tag[1..]
    );
    for (os, block) in [("darwin", "macos"), ("linux", "linux")] {
        out.push_str(&format!("  on_{block} do\n"));
        for (arch, brewarch) in [("arm64", "arm"), ("amd64", "intel")] {
            let name = name(os, arch)?;
            let sha = sums.get(&name).ok_or("missing artifact checksum")?;
            if sha.len() != 64
                || !sha
                    .bytes()
                    .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
            {
                return Err("invalid checksum".into());
            }
            out.push_str(&format!("    on_{brewarch} do\n      url \"{base}/{name}\"\n      sha256 \"{sha}\"\n    end\n"));
        }
        out.push_str("  end\n\n");
    }
    out.push_str("  def install\n    bin.install Dir[\"oflh-*\"][0] => \"oflh\"\n  end\n\n  test do\n    assert_match \"oflh #{version}\", shell_output(\"#{bin}/oflh --version\")\n  end\nend\n");
    Ok(out)
}
fn assemble(output: &Path, tag: &str, base: Option<&str>) -> Result<String> {
    version(tag)?;
    let expected: Vec<_> = TARGETS
        .iter()
        .map(|(os, arch)| name(os, arch))
        .collect::<Result<_>>()?;
    for entry in fs::read_dir(output)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if name != "checksums.txt" && !expected.contains(&name) {
            return Err(format!("unexpected release artifact: {name}").into());
        }
    }
    let mut sums = BTreeMap::new();
    for name in expected {
        let p = output.join(&name);
        if !p.is_file() || p.metadata()?.len() == 0 {
            return Err(format!("missing tested artifact: {name}").into());
        }
        sums.insert(name, digest(&p)?);
    }
    let formula = formula(tag, &sums, base)?;
    let mut checksum = File::create(output.join("checksums.txt"))?;
    for (name, digest) in &sums {
        writeln!(checksum, "{digest}  {name}")?;
    }
    Ok(formula)
}
fn run() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let action = args
        .next()
        .ok_or("usage: cargo xtask validate|package|assemble --version TAG [options]")?;
    let mut options = BTreeMap::new();
    while let Some(key) = args.next() {
        if ![
            "--version",
            "--os",
            "--arch",
            "--binary",
            "--output",
            "--formula",
            "--base-url",
        ]
        .contains(&key.as_str())
        {
            return Err(format!("unknown option {key}").into());
        }
        let value = args.next().ok_or("option requires a value")?;
        if options.insert(key, value).is_some() {
            return Err("duplicate option".into());
        }
    }
    let required = |name: &str| {
        options
            .get(name)
            .map(String::as_str)
            .ok_or_else(|| format!("missing {name}"))
    };
    let tag = required("--version")?;
    version(tag)?;
    let output = Path::new(options.get("--output").map_or("dist", String::as_str));
    match action.as_str() {
        "validate" => {}
        "package" => {
            package(
                Path::new(required("--binary")?),
                output,
                tag,
                required("--os")?,
                required("--arch")?,
            )?;
        }
        "assemble" => {
            let f = assemble(output, tag, options.get("--base-url").map(String::as_str))?;
            if let Some(path) = options.get("--formula") {
                fs::write(path, f)?
            } else {
                print!("{f}")
            }
        }
        _ => return Err("unknown action".into()),
    }
    Ok(())
}
fn main() -> std::process::ExitCode {
    match run() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("release: {e}");
            std::process::ExitCode::FAILURE
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn versions() {
        for v in ["dev", "v0.0.10-rc.1", "v1.2.3"] {
            assert!(version(v).is_ok())
        }
        for v in ["1.2.3", "v1.2", "v1.2.3-", "v01.2.3", "v1.2.3\n"] {
            assert!(version(v).is_err())
        }
    }
    #[test]
    fn artifacts() {
        let root = std::env::temp_dir().join(format!("oflh-package-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let source = root.join("binary");
        fs::write(&source, b"test payload").unwrap();
        let output = root.join("dist");
        for (os, arch) in TARGETS {
            let p = package(&source, &output, "v0.0.10-rc.1", os, arch).unwrap();
            assert_eq!(fs::read(p).unwrap(), b"test payload")
        }
        let f = assemble(&output, "v0.0.10-rc.1", None).unwrap();
        assert_eq!(f.matches("sha256").count(), 4);
        assert!(f.contains("releases/download/v0.0.10-rc.1/oflh-darwin-arm64"));
        assert_eq!(
            fs::read_to_string(output.join("checksums.txt"))
                .unwrap()
                .lines()
                .count(),
            6
        );
        fs::write(output.join("unexpected"), b"bad").unwrap();
        assert!(assemble(&output, "v0.0.10-rc.1", None).is_err());
        fs::remove_file(output.join("unexpected")).unwrap();
        fs::remove_file(output.join("oflh-linux-arm64")).unwrap();
        assert!(assemble(&output, "v0.0.10-rc.1", None).is_err());
        fs::remove_dir_all(root).unwrap();
    }
}
