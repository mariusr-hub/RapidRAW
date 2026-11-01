//! Atomic file staging and commit utilities.
//!
//! Guarantees that export or stitched outputs are never partially written or corrupted
//! on unexpected cancellation, process termination, or disk errors.
//!
//! Staging temp files are explicitly created in the destination's parent directory
//! to avoid EXDEV (cross-device link) errors during filesystem atomic renames.

use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use uuid::Uuid;

pub struct AtomicFileWriter {
    temp_path: PathBuf,
    dest_path: PathBuf,
    file: Option<File>,
}

impl AtomicFileWriter {
    pub fn new<P: AsRef<Path>>(dest_path: P) -> io::Result<Self> {
        let dest = dest_path.as_ref().to_path_buf();
        let parent = dest.parent().unwrap_or_else(|| Path::new("."));

        // Stage in the SAME directory to guarantee atomic fs::rename without cross-device errors
        let file_stem = dest.file_name().unwrap_or_default().to_string_lossy();
        let temp_name = format!(".tmp_{}_{}", Uuid::new_v4(), file_stem);
        let temp_path = parent.join(temp_name);
        let file = File::create(&temp_path)?;

        Ok(Self {
            temp_path,
            dest_path: dest,
            file: Some(file),
        })
    }

    pub fn temp_path(&self) -> &Path {
        &self.temp_path
    }

    pub fn file_mut(&mut self) -> io::Result<&mut File> {
        self.file
            .as_mut()
            .ok_or_else(|| io::Error::new(io::ErrorKind::Other, "AtomicFileWriter already committed"))
    }

    pub fn write_all(&mut self, buf: &[u8]) -> io::Result<()> {
        let f = self.file_mut()?;
        f.write_all(buf)
    }

    pub fn flush(&mut self) -> io::Result<()> {
        let f = self.file_mut()?;
        f.flush()
    }

    pub fn commit(mut self) -> io::Result<()> {
        if let Some(mut file) = self.file.take() {
            file.flush()?;
            file.sync_all()?;
        }
        // Atomic replace in filesystem directory entry
        fs::rename(&self.temp_path, &self.dest_path)
    }
}

impl Write for AtomicFileWriter {
    #[inline]
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let f = self.file_mut()?;
        f.write(buf)
    }

    #[inline]
    fn flush(&mut self) -> io::Result<()> {
        let f = self.file_mut()?;
        f.flush()
    }
}

/// Helper function to atomically write an entire byte slice to a file path.
pub fn atomic_write_bytes<P: AsRef<Path>>(path: P, bytes: &[u8]) -> io::Result<()> {
    let mut writer = AtomicFileWriter::new(path)?;
    writer.write_all(bytes)?;
    writer.commit()
}

impl Drop for AtomicFileWriter {
    fn drop(&mut self) {
        // If file is still Some, commit() was not called (e.g. error or panic occurred)
        if self.file.is_some() {
            let _ = fs::remove_file(&self.temp_path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    #[test]
    fn test_atomic_file_writer_commit() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("final_image.tif");

        {
            let mut writer = AtomicFileWriter::new(&target).unwrap();
            assert!(writer.temp_path().exists(), "Temp staging file must exist");
            assert_eq!(
                writer.temp_path().parent().unwrap(),
                target.parent().unwrap(),
                "Must be in the same parent directory"
            );
            writer.write_all(b"ATOMIC_TEST_PAYLOAD").unwrap();
            writer.commit().unwrap();
        }

        assert!(target.exists(), "Target file must exist after commit");
        let mut content = Vec::new();
        File::open(&target).unwrap().read_to_end(&mut content).unwrap();
        assert_eq!(&content, b"ATOMIC_TEST_PAYLOAD");
    }

    #[test]
    fn test_atomic_file_writer_drop_cleanup() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("uncommitted.tif");
        let temp_staged;

        {
            let mut writer = AtomicFileWriter::new(&target).unwrap();
            temp_staged = writer.temp_path().to_path_buf();
            assert!(temp_staged.exists());
            writer.write_all(b"PARTIAL_DATA").unwrap();
            // Writer dropped here without commit
        }

        assert!(!target.exists(), "Target file must not exist");
        assert!(!temp_staged.exists(), "Temp staging file must be cleaned up on drop");
    }

    #[test]
    fn test_atomic_file_writer_overwrite_existing() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("existing.tif");
        fs::write(&target, b"OLD_DATA").unwrap();
        assert_eq!(fs::read(&target).unwrap(), b"OLD_DATA");

        {
            let mut writer = AtomicFileWriter::new(&target).unwrap();
            writer.write_all(b"NEW_ATOMIC_DATA").unwrap();
            writer.commit().unwrap();
        }

        assert_eq!(fs::read(&target).unwrap(), b"NEW_ATOMIC_DATA");
    }
}
