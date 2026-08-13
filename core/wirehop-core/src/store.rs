//! Non-overwriting file commit for received transfers.
//!
//! Local policy, not wire format: the protocol says nothing about where bytes
//! land. The invariant this upholds is the one from `PROTOCOL.md` §"Receiver
//! validation of metadata" — data is written to a temporary file **inside the
//! destination directory** and committed by rename, so an aborted transfer
//! never leaves a file at the final name and an existing file is never
//! overwritten.
//!
//! Same-directory placement is deliberate: a temp file elsewhere would make
//! the commit a cross-filesystem copy, which is neither atomic nor free.

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::Error;

/// How many numbered variants to try before giving up on a colliding name.
const MAX_COLLISION_ATTEMPTS: u32 = 10_000;

/// A partially received file, held at a temporary name until it is complete.
pub struct PartialFile {
    file: File,
    temp_path: PathBuf,
    committed: bool,
}

impl PartialFile {
    /// Creates a temporary file inside `dir`.
    pub fn create(dir: &Path, seed: u32) -> Result<Self, Error> {
        for attempt in 0..MAX_COLLISION_ATTEMPTS {
            let temp_path = dir.join(format!(".wirehop-part-{}-{}", seed, attempt));
            match OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temp_path)
            {
                Ok(file) => {
                    return Ok(Self {
                        file,
                        temp_path,
                        committed: false,
                    })
                }
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(Error::Io(e)),
            }
        }
        Err(Error::Protocol("cannot create a temporary file"))
    }

    pub fn write_all(&mut self, data: &[u8]) -> Result<(), Error> {
        self.file.write_all(data).map_err(Error::Io)
    }

    /// Renames the temporary file to `filename`, or to a numbered variant when
    /// that name is taken. Returns the path actually used.
    ///
    /// `filename` must already have passed `policy::is_safe_filename`; this
    /// function does not re-validate and must never be handed raw peer input.
    pub fn commit(mut self, dir: &Path, filename: &str) -> Result<PathBuf, Error> {
        self.file.flush().map_err(Error::Io)?;
        self.file.sync_all().map_err(Error::Io)?;

        for index in 0..MAX_COLLISION_ATTEMPTS {
            let candidate = dir.join(variant_name(filename, index));
            if candidate.exists() {
                continue;
            }
            // rename(2) would clobber an entry created between the check and
            // the call; create_new reserves the name first so the race cannot
            // destroy an existing file.
            match OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&candidate)
            {
                Ok(_) => {
                    std::fs::rename(&self.temp_path, &candidate).map_err(Error::Io)?;
                    self.committed = true;
                    return Ok(candidate);
                }
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(Error::Io(e)),
            }
        }
        Err(Error::Protocol("cannot find a free destination name"))
    }
}

impl Drop for PartialFile {
    /// An uncommitted partial file is removed, so an aborted transfer leaves
    /// nothing behind.
    fn drop(&mut self) {
        if !self.committed {
            let _ = std::fs::remove_file(&self.temp_path);
        }
    }
}

/// `report.pdf` → `report.pdf`, `report (1).pdf`, `report (2).pdf`, …
fn variant_name(filename: &str, index: u32) -> String {
    if index == 0 {
        return filename.to_string();
    }
    match filename.rfind('.') {
        // A leading dot is part of the name, not an extension separator.
        Some(dot) if dot > 0 => {
            format!("{} ({}){}", &filename[..dot], index, &filename[dot..])
        }
        _ => format!("{} ({})", filename, index),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tempdir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "wirehop-store-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn names_variants_around_the_extension() {
        assert_eq!(variant_name("a.txt", 0), "a.txt");
        assert_eq!(variant_name("a.txt", 1), "a (1).txt");
        assert_eq!(variant_name("archive.tar.gz", 2), "archive.tar (2).gz");
        assert_eq!(variant_name("noext", 3), "noext (3)");
        assert_eq!(variant_name(".hidden", 1), ".hidden (1)");
    }

    #[test]
    fn commits_to_the_declared_name_when_free() {
        let dir = tempdir();
        let mut partial = PartialFile::create(&dir, 0).unwrap();
        partial.write_all(b"payload").unwrap();
        let path = partial.commit(&dir, "a.txt").unwrap();

        assert_eq!(path, dir.join("a.txt"));
        assert_eq!(std::fs::read(&path).unwrap(), b"payload");
    }

    #[test]
    fn never_overwrites_an_existing_file() {
        let dir = tempdir();
        std::fs::write(dir.join("a.txt"), b"original").unwrap();

        let mut partial = PartialFile::create(&dir, 1).unwrap();
        partial.write_all(b"incoming").unwrap();
        let path = partial.commit(&dir, "a.txt").unwrap();

        assert_eq!(path, dir.join("a (1).txt"));
        assert_eq!(std::fs::read(dir.join("a.txt")).unwrap(), b"original");
        assert_eq!(std::fs::read(&path).unwrap(), b"incoming");
    }

    #[test]
    fn dropping_an_uncommitted_partial_leaves_nothing_behind() {
        let dir = tempdir();
        {
            let mut partial = PartialFile::create(&dir, 2).unwrap();
            partial.write_all(b"abandoned").unwrap();
        }
        let leftovers: Vec<_> = std::fs::read_dir(&dir).unwrap().collect();
        assert!(leftovers.is_empty(), "temp file survived drop");
    }
}
