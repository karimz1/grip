use oflh_core::*;
#[test]
fn escaping() {
    assert_eq!(safe("ok\x1b[31m\n\u{202e}"), "ok�[31m��");
}
#[test]
fn paths() {
    let root = std::env::temp_dir().join(format!("oflh-core-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let root = std::fs::canonicalize(root).unwrap();
    let p = root.join("file ü");
    std::fs::write(&p, b"data").unwrap();
    let target = Target::new(&p).unwrap();
    assert!(!target.directory);
    assert!(target.matches(&p, std::fs::metadata(&p).ok().as_ref()));
    let dir = Target::new(&root).unwrap();
    assert!(dir.contains(&p));
    assert!(!dir.contains(&root.with_extension("other")));
    assert!(
        Target::new(root.join("missing"))
            .unwrap()
            .metadata
            .is_none()
    );
    #[cfg(unix)]
    {
        let alias = root.join("alias");
        std::fs::hard_link(&p, &alias).unwrap();
        assert!(target.matches(&alias, std::fs::metadata(&alias).ok().as_ref()));
        let link = root.join("link");
        std::os::unix::fs::symlink(&p, &link).unwrap();
        assert_eq!(Target::new(link).unwrap().path, target.path);
    }
    std::fs::remove_dir_all(root).unwrap();
}
