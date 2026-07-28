//! Repository-relative support shared by unit specifications.

pub fn repository_root() -> std::path::PathBuf {
    let current = std::env::current_dir().expect("test process current directory");
    current
        .ancestors()
        .find(|candidate| {
            let Ok(manifest) = std::fs::read_to_string(candidate.join("Cargo.toml")) else {
                return false;
            };
            manifest.starts_with("[package]\nname = \"typokat\"\n")
                && candidate.join("Cargo.lock").is_file()
                && candidate.join("src/lib.rs").is_file()
        })
        .map(std::path::Path::to_path_buf)
        .expect("test process must run within the typokat repository")
}

pub fn rust_sources(root: &std::path::Path) -> Vec<std::path::PathBuf> {
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

pub fn workspace_source_roots() -> Vec<std::path::PathBuf> {
    let root = repository_root();
    let mut source_roots = vec![root.join("src")];
    let mut members = std::fs::read_dir(root.join("crates"))
        .expect("workspace crates directory")
        .map(|entry| entry.expect("workspace member entry").path())
        .filter(|path| path.is_dir() && path.join("src").is_dir())
        .map(|path| path.join("src"))
        .collect::<Vec<_>>();
    members.sort();
    source_roots.extend(members);
    source_roots
}

pub fn workspace_rust_sources() -> Vec<std::path::PathBuf> {
    workspace_source_roots()
        .into_iter()
        .flat_map(|root| rust_sources(&root))
        .collect()
}
