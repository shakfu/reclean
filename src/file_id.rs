//! File identity on Windows: volume serial number and 128-bit file ID.
//!
//! Std exposes these only behind the unstable `windows_by_handle` feature.
//! Creation time is no substitute: NTFS tunneling gives a name recreated
//! within seconds the creation time of the entry it replaced.

use std::fs::OpenOptions;
use std::io;
use std::mem::{size_of, MaybeUninit};
use std::os::windows::fs::OpenOptionsExt;
use std::os::windows::io::AsRawHandle;
use std::path::Path;

use windows_sys::Win32::Storage::FileSystem::{
    FileIdInfo, GetFileInformationByHandleEx, FILE_FLAG_BACKUP_SEMANTICS,
    FILE_FLAG_OPEN_REPARSE_POINT, FILE_ID_INFO,
};

/// Volume serial number and file ID
pub(crate) type FileId = (u64, [u8; 16]);

/// Identity of `path`, without following a final symlink or junction
pub(crate) fn file_id(path: &Path) -> io::Result<FileId> {
    // Access mode 0 queries attributes only; backup semantics opens directories
    let file = OpenOptions::new()
        .access_mode(0)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)?;
    let mut info = MaybeUninit::<FILE_ID_INFO>::uninit();
    // SAFETY: the handle is open for the call, and the buffer is a
    // FILE_ID_INFO of the size passed.
    let ok = unsafe {
        GetFileInformationByHandleEx(
            file.as_raw_handle(),
            FileIdInfo,
            info.as_mut_ptr().cast(),
            size_of::<FILE_ID_INFO>() as u32,
        )
    };
    if ok == 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: the call succeeded, so it filled the buffer
    let info = unsafe { info.assume_init() };
    Ok((info.VolumeSerialNumber, info.FileId.Identifier))
}
