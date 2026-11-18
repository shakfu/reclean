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
//!
//! A large directory target is emptied by all cores (`empty_split`).

use std::ffi::{CStr, CString, OsStr};
use std::fs::Metadata;
use std::io;
use std::mem::MaybeUninit;
use std::os::fd::{AsFd, AsRawFd, BorrowedFd, FromRawFd, IntoRawFd, OwnedFd, RawFd};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::MetadataExt;
use std::path::{Component, Path};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::OnceLock;

/// Remove `rel`, a path below `root`, if it is still the object `scanned`
/// describes. `on_unlink` is called after each entry is removed. With `split`,
/// a directory is emptied across all cores.
pub(crate) fn remove(
    root: &Path,
    rel: &Path,
    scanned: &Metadata,
    split: bool,
    on_unlink: &(dyn Fn() + Sync),
) -> Result<(), String> {
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
        if split {
            empty_split(&target, on_unlink)
        } else {
            empty(&target, on_unlink)
        }
        .map_err(|e| e.to_string())?;
        drop(target);
        unlink(dir.as_raw_fd(), last, libc::AT_REMOVEDIR).map_err(|e| e.to_string())?;
    } else {
        if !same_object(
            scanned,
            &stat_at(dir.as_raw_fd(), last).map_err(|e| e.to_string())?,
        ) {
            return Err(replaced());
        }
        unlink(dir.as_raw_fd(), last, 0).map_err(|e| e.to_string())?;
    }
    on_unlink();
    Ok(())
}

/// Remove everything inside the directory open as `dir`, without following
/// any symlink in it.
fn empty(dir: &OwnedFd, on_unlink: &dyn Fn()) -> io::Result<()> {
    // Names are read in full before any is removed: removing entries while a
    // directory stream is open can make it skip others.
    for (name, kind) in entries(dir.as_fd())? {
        let is_dir = is_dir_entry(dir.as_raw_fd(), &name, kind)?;
        remove_entry(dir.as_raw_fd(), &name, is_dir, on_unlink)?;
    }
    Ok(())
}

/// Remove `name` in `dir`, emptying it first if it is a directory.
fn remove_entry(dir: RawFd, name: &CStr, is_dir: bool, on_unlink: &dyn Fn()) -> io::Result<()> {
    if is_dir {
        let child = open_dir(dir, name, false)?;
        empty(&child, on_unlink)?;
        drop(child);
        unlink(dir, name, libc::AT_REMOVEDIR)?;
    } else {
        unlink(dir, name, 0)?;
    }
    on_unlink();
    Ok(())
}

fn is_dir_entry(dir: RawFd, name: &CStr, kind: u8) -> io::Result<bool> {
    Ok(match kind {
        libc::DT_DIR => true,
        libc::DT_UNKNOWN => is_dir_mode(stat_at(dir, name)?.st_mode),
        _ => false,
    })
}

/// A directory opened by `empty_split`, removed once everything below it is.
struct Held {
    // Kept open: `parent` of the level below is this descriptor
    _fd: OwnedFd,
    parent: RawFd,
    name: CString,
}

/// An entry for a worker to remove, recursively if it is a directory.
struct Job {
    parent: RawFd,
    name: CString,
    is_dir: bool,
}

/// `empty` spread across all cores, for one large target. Targets already run
/// in parallel, which leaves a single large one on one thread.
///
/// The top levels are opened breadth first until there are about 4
/// subdirectories per thread. Every entry of the last level read becomes a job,
/// files included, since a flat directory such as Cargo's `target/debug/deps`
/// holds most of a tree. Workers descend only through descriptors held here, so
/// no symlink is followed. Held directories are removed deepest first after
/// every job ends, so none is removed while a worker is below it. Ported from
/// cclean's `empty_directory_split`.
fn empty_split(dir: &OwnedFd, on_unlink: &(dyn Fn() + Sync)) -> io::Result<()> {
    // The descriptor cap stays well inside the common limit of 1024
    const MAX_HELD: usize = 256;
    const MAX_DEPTH: usize = 4;
    let threads = std::thread::available_parallelism().map_or(1, |n| n.get());

    let mut held: Vec<Held> = Vec::new();
    let mut jobs: Vec<Job> = Vec::new();
    // Descriptors stay owned by `dir` or `held` until this function returns
    let mut level: Vec<RawFd> = vec![dir.as_raw_fd()];
    for depth in 1.. {
        let mut subdirs = Vec::new();
        for &fd in &level {
            // SAFETY: `fd` is owned by `dir` or `held`, both alive here.
            for (name, kind) in entries(unsafe { BorrowedFd::borrow_raw(fd) })? {
                if is_dir_entry(fd, &name, kind)? {
                    subdirs.push((fd, name));
                } else {
                    jobs.push(Job {
                        parent: fd,
                        name,
                        is_dir: false,
                    });
                }
            }
        }
        if subdirs.len() >= 4 * threads
            || depth == MAX_DEPTH
            || held.len() + subdirs.len() > MAX_HELD
        {
            jobs.extend(subdirs.into_iter().map(|(parent, name)| Job {
                parent,
                name,
                is_dir: true,
            }));
            break;
        }
        level.clear();
        for (parent, name) in subdirs {
            match open_dir(parent, &name, false) {
                Ok(fd) => {
                    level.push(fd.as_raw_fd());
                    held.push(Held {
                        _fd: fd,
                        parent,
                        name,
                    });
                }
                // The job's own removal reports why it cannot be opened
                Err(_) => jobs.push(Job {
                    parent,
                    name,
                    is_dir: true,
                }),
            }
        }
        if level.is_empty() {
            break;
        }
    }

    // One slot per job; the first failure in job order is reported, so the
    // message does not depend on scheduling.
    let errors: Vec<OnceLock<io::Error>> = jobs.iter().map(|_| OnceLock::new()).collect();
    let next = AtomicUsize::new(0);
    std::thread::scope(|scope| {
        for _ in 0..threads.min(jobs.len()) {
            scope.spawn(|| loop {
                let i = next.fetch_add(1, Ordering::Relaxed);
                let Some(job) = jobs.get(i) else {
                    return;
                };
                if let Err(e) = remove_entry(job.parent, &job.name, job.is_dir, on_unlink) {
                    let _ = errors[i].set(e);
                }
            });
        }
    });
    let mut first = errors.into_iter().find_map(OnceLock::into_inner);

    // Held directories go even after a failure; only those above it remain
    for h in held.iter().rev() {
        match unlink(h.parent, &h.name, libc::AT_REMOVEDIR) {
            Ok(()) => on_unlink(),
            Err(e) => {
                first.get_or_insert(e);
            }
        }
    }
    first.map_or(Ok(()), Err)
}

/// The names in `dir` other than `.` and `..`, with their `d_type`. A read
/// error ends the listing early; the directory then fails to unlink, which
/// reports it.
fn entries(dir: BorrowedFd) -> io::Result<Vec<(CString, u8)>> {
    // fdopendir takes ownership of its descriptor, so it gets a duplicate.
    let raw = dir.try_clone_to_owned()?.into_raw_fd();
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
