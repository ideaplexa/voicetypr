//! Blocking recovery commits only, never used by the real-time audio writer.
use std::{io, path::Path};

pub(super) fn sync_file(path: &Path) -> io::Result<()> {
    std::fs::OpenOptions::new()
        .read(true)
        .write(cfg!(windows))
        .open(path)?
        .sync_all()
}

pub(super) fn sync_parent(path: &Path) -> io::Result<()> {
    #[cfg(unix)]
    {
        std::fs::File::open(path.parent().ok_or(io::ErrorKind::InvalidInput)?)?.sync_all()
    }
    #[cfg(not(unix))]
    {
        // Windows replacement uses MOVEFILE_WRITE_THROUGH below. Opening a
        // directory and calling FlushFileBuffers is not supported there.
        let _ = path;
        Ok(())
    }
}

fn rename(source: &Path, target: &Path) -> io::Result<()> {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        use windows::{
            core::PCWSTR,
            Win32::Storage::FileSystem::{
                MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
            },
        };
        let source: Vec<u16> = source.as_os_str().encode_wide().chain(Some(0)).collect();
        let target: Vec<u16> = target.as_os_str().encode_wide().chain(Some(0)).collect();
        unsafe {
            MoveFileExW(
                PCWSTR(source.as_ptr()),
                PCWSTR(target.as_ptr()),
                MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
            )
        }
        .map_err(io::Error::other)
    }
    #[cfg(not(windows))]
    std::fs::rename(source, target)
}

pub(super) fn replace_index(file: tempfile::NamedTempFile, target: &Path) -> io::Result<()> {
    file.as_file().sync_all()?;
    let source = file.into_temp_path();
    rename(&source, target)?;
    sync_parent(target)
}

/// Startup only, after the old process/writer is gone. Repair a separate copy
/// of our recorder's PCM16 WAV, preserving the original on every failure.
pub(super) fn recover_interrupted_capture(source: &Path, target: &Path) -> io::Result<()> {
    use io::{Read, Seek, SeekFrom, Write};
    if source == target {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    let reader = hound::WavReader::new(std::fs::File::open(source)?).map_err(io::Error::other)?;
    let spec = reader.spec();
    if spec.bits_per_sample != 16
        || spec.sample_format != hound::SampleFormat::Int
        || spec.channels == 0
    {
        return Err(io::ErrorKind::InvalidData.into());
    }
    let mut input = reader.into_inner();
    let data_start = input.stream_position()?;
    let frame_bytes = u64::from(spec.channels) * 2;
    let available = input
        .metadata()?
        .len()
        .checked_sub(data_start)
        .ok_or(io::ErrorKind::InvalidData)?;
    let data_len = available / frame_bytes * frame_bytes;
    let end = data_start + data_len;
    if data_start < 8 || data_len == 0 || end - 8 > u64::from(u32::MAX) {
        return Err(io::ErrorKind::InvalidData.into());
    }
    let mut file = tempfile::Builder::new()
        .prefix(".index-")
        .tempfile_in(target.parent().ok_or(io::ErrorKind::InvalidInput)?)?;
    input.seek(SeekFrom::Start(0))?;
    if io::copy(&mut input.take(end), &mut file)? != end {
        return Err(io::ErrorKind::UnexpectedEof.into());
    }
    file.as_file_mut().set_len(end)?;
    file.seek(SeekFrom::Start(data_start - 4))?;
    file.write_all(&(data_len as u32).to_le_bytes())?;
    file.seek(SeekFrom::Start(4))?;
    file.write_all(&((end - 8) as u32).to_le_bytes())?;
    file.flush()?;
    replace_index(file, target)?;
    let _ = unlink_committed(source);
    Ok(())
}

pub(super) fn unlink_committed(path: &Path) -> bool {
    (std::fs::remove_file(path).is_ok()
        || std::fs::symlink_metadata(path)
            .err()
            .is_some_and(|e| e.kind() == io::ErrorKind::NotFound))
        && sync_parent(path).is_ok()
}

pub(super) fn move_audio(source: &Path, target: &Path) -> bool {
    // Flush sample data and the finalized header before publishing ownership.
    if sync_file(source).is_err() {
        return false;
    }
    if rename(source, target).is_ok() {
        // Even if the directory flush fails, ownership has already moved.
        // Index publication retries the flush and reports a storage failure.
        let _ = sync_parent(target);
        let _ = sync_parent(source);
        return true;
    }
    copy_audio(source, target)
}

fn copy_audio(source: &Path, target: &Path) -> bool {
    // A cross-volume copy must be durable before the only original is unlinked.
    if std::fs::copy(source, target).is_err()
        || sync_file(target).is_err()
        || sync_parent(target).is_err()
    {
        let _ = std::fs::remove_file(target);
        return false;
    }
    // Failure to unlink leaves an extra original, never loses the durable copy.
    if std::fs::remove_file(source).is_ok() {
        let _ = sync_parent(source);
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copy_failure_keeps_the_original_and_success_publishes_all_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("source");
        let target = dir.path().join("target");
        std::fs::write(&source, b"finalized samples").unwrap();
        std::fs::create_dir(&target).unwrap();
        assert!(!copy_audio(&source, &target));
        assert_eq!(std::fs::read(&source).unwrap(), b"finalized samples");
        std::fs::remove_dir(&target).unwrap();
        assert!(copy_audio(&source, &target));
        assert_eq!(std::fs::read(&target).unwrap(), b"finalized samples");
        assert!(!source.exists());
    }
}
