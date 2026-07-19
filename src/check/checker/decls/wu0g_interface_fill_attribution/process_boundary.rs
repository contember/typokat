use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::Path;

pub(super) fn read_bounded_file(path: &Path, cap: u64) -> Result<Vec<u8>, String> {
    let mut file = OpenOptions::new()
        .read(true)
        .open(path)
        .map_err(|error| format!("bounded open failed for {}: {error}", path.display()))?;
    let take = cap
        .checked_add(1)
        .ok_or_else(|| "bounded read cap overflow".to_owned())?;
    let mut bytes = Vec::new();
    Read::by_ref(&mut file)
        .take(take)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("bounded read failed for {}: {error}", path.display()))?;
    if u64::try_from(bytes.len()).map_err(|_| "bounded read length does not fit u64")? > cap {
        return Err(format!("bounded read exceeded cap for {}", path.display()));
    }
    Ok(bytes)
}

pub(super) fn write_exclusive_bounded(path: &Path, bytes: &[u8], cap: u64) -> Result<(), String> {
    if u64::try_from(bytes.len()).map_err(|_| "output length does not fit u64")? > cap {
        return Err(format!("output exceeds cap for {}", path.display()));
    }
    let mut file = create_exclusive_file(path)?;
    file.write_all(bytes)
        .and_then(|_| file.sync_all())
        .map_err(|error| format!("output write failed for {}: {error}", path.display()))
}

pub(super) fn create_exclusive_file(path: &Path) -> Result<File, String> {
    let parent = path
        .parent()
        .ok_or_else(|| "output path lacks parent".to_owned())?;
    let metadata = std::fs::symlink_metadata(parent)
        .map_err(|error| format!("output parent metadata failed: {error}"))?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err("output parent is not a real directory".to_owned());
    }
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| {
            format!(
                "exclusive output create failed for {}: {error}",
                path.display()
            )
        })
}
