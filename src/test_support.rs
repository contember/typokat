//! Repository-relative support shared by unit specifications.

pub(crate) fn repository_root() -> std::path::PathBuf {
    let root = std::env::current_dir().expect("test process current directory");
    let manifest = std::fs::read_to_string(root.join("Cargo.toml")).expect("repository Cargo.toml");
    assert!(
        manifest.starts_with("[package]\nname = \"typokat\"\n"),
        "tests must run from the typokat repository root"
    );
    assert!(
        root.join("Cargo.lock").is_file() && root.join("src/lib.rs").is_file(),
        "repository root sentinels must exist"
    );
    root
}

pub(crate) fn rust_sources(root: &std::path::Path) -> Vec<std::path::PathBuf> {
    if root.is_file() {
        return vec![root.to_path_buf()];
    }
    let mut pending = vec![root.to_path_buf()];
    let mut sources = Vec::new();
    while let Some(directory) = pending.pop() {
        let mut entries = std::fs::read_dir(directory)
            .expect("source directory")
            .map(|entry| entry.expect("source entry").path())
            .collect::<Vec<_>>();
        entries.sort();
        for path in entries {
            if path.is_dir() {
                pending.push(path);
            } else if path.extension().and_then(|extension| extension.to_str()) == Some("rs") {
                sources.push(path);
            }
        }
    }
    sources.sort();
    sources
}
