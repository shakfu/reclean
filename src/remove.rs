//! Descriptor-relative removal, on Unix.
//!
//! The root is opened by name and followed, since the user named it. Every
//! component below it is opened with `O_NOFOLLOW` through the descriptor of the
//! one above, and every removal names an entry within a directory descriptor.
//! A symlink substituted for a directory is therefore an error rather than a
//! way out of the root, and a component renamed after the scan cannot be
//! reached by name at all.
//!
//! The target is then confirmed to be the object the scan found, by device and
//! inode. A directory is checked through `fstat` on its open descriptor and
//! emptied through that descriptor. Anything else is checked with `fstatat`
//! and unlinked by name immediately after, which narrows the window to two
//! adjacent syscalls rather than closing it.

use std::ffi::{CStr, CString, OsStr};
use std::fs::Metadata;
use std::io;
use std::mem::MaybeUninit;
use std::os::fd::{AsRawFd, FromRawFd, IntoRawFd, OwnedFd, RawFd};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::MetadataExt;
use std::path::{Component, Path};

/// Remove `rel`, a path below `root`, if it is still the object `scanned`
/// describes.
pub(crate) fn remove(root: &Path, rel: &Path, scanned: &Metadata) -> Result<(), String> {
    let mut names = Vec::new();
    for component in rel.components() {
        match component {
            Component::Normal(name) => names.push(c_name(name)?),
            Component::CurDir => {}
            _ => return Err("is not below the working directory".to_string()),
        }
    }
    let Some((last, parents)) = names.split_last() else {
        return Err("is the working directory itself".to_string());
    };

    let mut dir = open_dir(libc::AT_FDCWD, &c_name(root.as_os_str())?, true)
        .map_err(|e| format!("cannot open {:?}: {}", root, e))?;
    for name in parents {
        dir = open_dir(dir.as_raw_fd(), name, false)
            .map_err(|e| format!("cannot open component {:?}: {}", name, e))?;
    }

    let replaced = || "replaced since it was scanned".to_string();
    if scanned.is_dir() {
        let target = open_dir(dir.as_raw_fd(), last, false).map_err(|e| e.to_string())?;
        if !same_object(
            scanned,
            &fstat(target.as_raw_fd()).map_err(|e| e.to_string())?,
        ) {
            return Err(replaced());
        }
        empty(&target).map_err(|e| e.to_string())?;
        drop(target);
        unlink(dir.as_raw_fd(), last, libc::AT_REMOVEDIR).map_err(|e| e.to_string())
    } else {
        if !same_object(
            scanned,
            &stat_at(dir.as_raw_fd(), last).map_err(|e| e.to_string())?,
        ) {
            return Err(replaced());
        }
        unlink(dir.as_raw_fd(), last, 0).map_err(|e| e.to_string())
    }
}

/// Remove everything inside the directory open as `dir`, without following
/// any symlink in it.
fn empty(dir: &OwnedFd) -> io::Result<()> {
    // Names are read in full before any is removed: removing entries while a
    // directory stream is open can make it skip others.
    for (name, kind) in entries(dir)? {
        let is_dir = match kind {
            libc::DT_DIR => true,
            libc::DT_UNKNOWN => is_dir_mode(stat_at(dir.as_raw_fd(), &name)?.st_mode),
            _ => false,
        };
        if is_dir {
            let child = open_dir(dir.as_raw_fd(), &name, false)?;
            empty(&child)?;
            drop(child);
            unlink(dir.as_raw_fd(), &name, libc::AT_REMOVEDIR)?;
        } else {
            unlink(dir.as_raw_fd(), &name, 0)?;
        }
    }
    Ok(())
}

/// The names in `dir` other than `.` and `..`, with their `d_type`. A read
/// error ends the listing early; the directory then fails to unlink, which
/// reports it.
fn entries(dir: &OwnedFd) -> io::Result<Vec<(CString, u8)>> {
    // fdopendir takes ownership of its descriptor, so it gets a duplicate.
    let raw = dir.try_clone()?.into_raw_fd();
    // SAFETY: `raw` is an open directory descriptor that nothing else owns.
    let stream = unsafe { libc::fdopendir(raw) };
    if stream.is_null() {
        let error = io::Error::last_os_error();
        // SAFETY: fdopendir failed, so `raw` is still ours to close.
        unsafe { libc::close(raw) };
        return Err(error);
    }

    let mut names = Vec::new();
    loop {
        // SAFETY: `stream` is a valid DIR* until closedir below.
        let entry = unsafe { libc::readdir(stream) };
        if entry.is_null() {
            break;
        }
        // SAFETY: readdir returned a valid entry whose name is NUL-terminated.
        let (name, kind) = unsafe { (CStr::from_ptr((*entry).d_name.as_ptr()), (*entry).d_type) };
        if name.to_bytes() != b"." && name.to_bytes() != b".." {
            names.push((name.to_owned(), kind));
        }
    }
    // SAFETY: `stream` is valid and not used afterwards; this also closes `raw`.
    unsafe { libc::closedir(stream) };
    Ok(names)
}

fn c_name(name: &OsStr) -> Result<CString, String> {
    CString::new(name.as_bytes()).map_err(|_| format!("{:?} contains a NUL byte", name))
}

/// Open `name` in `dir` as a directory. Without `follow`, a symlink is an error.
fn open_dir(dir: RawFd, name: &CStr, follow: bool) -> io::Result<OwnedFd> {
    let mut flags = libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC;
    if !follow {
        flags |= libc::O_NOFOLLOW;
    }
    // SAFETY: `name` is a valid C string and `dir` an open descriptor or AT_FDCWD.
    let fd = unsafe { libc::openat(dir, name.as_ptr(), flags) };
    if fd < 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: `fd` was just opened and nothing else owns it.
    Ok(unsafe { OwnedFd::from_raw_fd(fd) })
}

fn fstat(fd: RawFd) -> io::Result<libc::stat> {
    let mut st = MaybeUninit::<libc::stat>::uninit();
    // SAFETY: `fd` is open and `st` is writable storage for one stat.
    if unsafe { libc::fstat(fd, st.as_mut_ptr()) } < 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: fstat succeeded, so it filled `st`.
    Ok(unsafe { st.assume_init() })
}

fn stat_at(dir: RawFd, name: &CStr) -> io::Result<libc::stat> {
    let mut st = MaybeUninit::<libc::stat>::uninit();
    // SAFETY: as in `fstat`; AT_SYMLINK_NOFOLLOW reports a link, not its target.
    let r = unsafe {
        libc::fstatat(
            dir,
            name.as_ptr(),
            st.as_mut_ptr(),
            libc::AT_SYMLINK_NOFOLLOW,
        )
    };
    if r < 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: fstatat succeeded, so it filled `st`.
    Ok(unsafe { st.assume_init() })
}

fn unlink(dir: RawFd, name: &CStr, flags: libc::c_int) -> io::Result<()> {
    // SAFETY: `name` is a valid C string and `dir` an open directory descriptor.
    if unsafe { libc::unlinkat(dir, name.as_ptr(), flags) } < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

fn is_dir_mode(mode: libc::mode_t) -> bool {
    mode & libc::S_IFMT == libc::S_IFDIR
}

/// Whether `current` is the object `scanned` describes. An inode number can be
/// reused once freed, so the file type is compared too.
// dev_t, ino_t and mode_t differ in width and sign between platforms, so a
// cast that is a no-op here is a conversion elsewhere.
#[allow(clippy::unnecessary_cast)]
fn same_object(scanned: &Metadata, current: &libc::stat) -> bool {
    let kind = |mode: u32| mode & libc::S_IFMT as u32;
    scanned.dev() == current.st_dev as u64
        && scanned.ino() == current.st_ino as u64
        && kind(scanned.mode()) == kind(current.st_mode as u32)
}
