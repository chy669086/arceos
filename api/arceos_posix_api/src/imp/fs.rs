use alloc::boxed::Box;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec;
use alloc::vec::Vec;
use core::ffi::{c_char, c_int, c_void, CStr};

use axerrno::{LinuxError, LinuxResult};
use axfs::fops::OpenOptions;
use axio::{PollState, SeekFrom};
use axsync::Mutex;

use super::fd_ops::{get_file_like, FileLike};
use crate::{ctypes, utils::char_ptr_to_str};

pub struct File {
    inner: Mutex<axfs::fops::File>,
}

impl File {
    fn new(inner: axfs::fops::File) -> Self {
        Self {
            inner: Mutex::new(inner),
        }
    }

    fn add_to_fd_table(self) -> LinuxResult<c_int> {
        super::fd_ops::add_file_like(Arc::new(self))
    }

    fn from_fd(fd: c_int) -> LinuxResult<Arc<Self>> {
        let f = super::fd_ops::get_file_like(fd)?;
        f.into_any()
            .downcast::<Self>()
            .map_err(|_| LinuxError::EINVAL)
    }
}

impl FileLike for File {
    fn read(&self, buf: &mut [u8]) -> LinuxResult<usize> {
        Ok(self.inner.lock().read(buf)?)
    }

    fn write(&self, buf: &[u8]) -> LinuxResult<usize> {
        Ok(self.inner.lock().write(buf)?)
    }

    fn stat(&self) -> LinuxResult<ctypes::stat> {
        let metadata = self.inner.lock().get_attr()?;
        let ty = metadata.file_type() as u8;
        let perm = metadata.perm().bits() as u32;
        let st_mode = ((ty as u32) << 12) | perm;
        Ok(ctypes::stat {
            st_ino: 1,
            st_nlink: 1,
            st_mode,
            st_uid: 1000,
            st_gid: 1000,
            st_size: metadata.size() as _,
            st_blocks: metadata.blocks() as _,
            st_blksize: 512,
            ..Default::default()
        })
    }

    fn into_any(self: Arc<Self>) -> Arc<dyn core::any::Any + Send + Sync> {
        self
    }

    fn poll(&self) -> LinuxResult<PollState> {
        Ok(PollState {
            readable: true,
            writable: true,
        })
    }

    fn set_nonblocking(&self, _nonblocking: bool) -> LinuxResult {
        Ok(())
    }
}

pub struct Directory {
    inner: Mutex<axfs::fops::Directory>,
    path: String,
}

impl Directory {
    fn new(inner: axfs::fops::Directory, path: String) -> Self {
        Self {
            inner: Mutex::new(inner),
            path,
        }
    }

    fn from_path(path: String, options: &OpenOptions) -> LinuxResult<Self> {
        axfs::fops::Directory::open_dir(&path, options)
            .map_err(Into::into)
            .map(|d| Self::new(d, path))
    }

    fn add_to_fd_table(self) -> LinuxResult<c_int> {
        super::fd_ops::add_file_like(Arc::new(self))
    }

    pub fn from_fd(fd: c_int) -> LinuxResult<Arc<Self>> {
        let f = super::fd_ops::get_file_like(fd)?;
        f.into_any()
            .downcast::<Self>()
            .map_err(|_| LinuxError::EINVAL)
    }

    pub fn path(&self) -> &str {
        &self.path
    }
}

impl FileLike for Directory {
    fn read(&self, _buf: &mut [u8]) -> LinuxResult<usize> {
        Err(LinuxError::EBADF)
    }

    fn write(&self, _buf: &[u8]) -> LinuxResult<usize> {
        Err(LinuxError::EBADF)
    }

    fn stat(&self) -> LinuxResult<ctypes::stat> {
        Err(LinuxError::EBADF)
    }

    fn into_any(self: Arc<Self>) -> Arc<dyn core::any::Any + Send + Sync> {
        self
    }

    fn poll(&self) -> LinuxResult<PollState> {
        Ok(PollState {
            readable: true,
            writable: false,
        })
    }

    fn set_nonblocking(&self, _nonblocking: bool) -> LinuxResult {
        Ok(())
    }
}

/// Convert open flags to [`OpenOptions`].
fn flags_to_options(flags: c_int, _mode: ctypes::mode_t) -> OpenOptions {
    let flags = flags as u32;
    let mut options = OpenOptions::new();
    match flags & 0b11 {
        ctypes::O_RDONLY => options.read(true),
        ctypes::O_WRONLY => options.write(true),
        _ => {
            options.read(true);
            options.write(true);
        }
    };
    if flags & ctypes::O_APPEND != 0 {
        options.append(true);
    }
    if flags & ctypes::O_TRUNC != 0 {
        options.truncate(true);
    }
    if flags & ctypes::O_CREAT != 0 {
        options.create(true);
    }
    if flags & ctypes::O_EXEC != 0 {
        options.create_new(true);
        // options.execute(true);
    }
    if flags & ctypes::O_DIRECTORY != 0 {
        options.directory(true);
    }
    options
}

pub fn read_file(fd: c_int, offset: usize, size: usize) -> LinuxResult<Vec<u8>> {
    let file = get_file_like(fd)?;
    let file_size = file.stat()?.st_size as usize;
    let file = file
        .into_any()
        .downcast::<File>()
        .map_err(|_| LinuxError::EBADF)?;

    let file = file.inner.lock();
    if offset >= file_size {
        return Err(LinuxError::EINVAL);
    }
    let size = core::cmp::min(size, file_size - offset);

    let mut buf = vec![0u8; size];
    file.read_at(offset as u64, &mut buf)?;

    Ok(buf)
}

/// Open a file by `filename` and insert it into the file descriptor table.
///
/// Return its index in the file table (`fd`). Return `EMFILE` if it already
/// has the maximum number of files open.
pub fn sys_open(filename: *const c_char, flags: c_int, mode: ctypes::mode_t) -> c_int {
    let filename = char_ptr_to_str(filename);
    debug!("sys_open <= {:?} {:#o} {:#o}", filename, flags, mode);
    syscall_body!(sys_open, {
        let options = flags_to_options(flags, mode);
        if options.has_directory() {
            return Directory::from_path(filename?.into(), &options)?.add_to_fd_table();
        }
        add_file_or_directory_fd(
            axfs::fops::File::open,
            axfs::fops::Directory::open_dir,
            filename?,
            &options,
        )
    })
}

pub fn sys_openat(
    dirfd: c_int,
    filename: *const c_char,
    flags: c_int,
    mode: ctypes::mode_t,
) -> c_int {
    let filename = char_ptr_to_str(filename);
    debug!(
        "sys_openat <= {} {:?} {:#o} {:#o}",
        dirfd, filename, flags, mode
    );

    let Ok(filename) = filename else {
        return -1;
    };

    let options = flags_to_options(flags, mode);

    if filename.starts_with('/') || dirfd == -100 {
        return sys_open(filename.as_ptr() as _, flags, mode);
    }

    syscall_body!(sys_openat, {
        let dir = Directory::from_fd(dirfd)?;
        add_file_or_directory_fd(
            |filename, options| dir.inner.lock().open_file_at(filename, options),
            |filename, options| dir.inner.lock().open_dir_at(filename, options),
            filename,
            &options,
        )
    })
}

/// Use the function to open file or directory, then add into file descriptor table.
/// First try opening files, if fails, try directory.
fn add_file_or_directory_fd<F, D, E>(
    open_file: F,
    open_dir: D,
    filename: &str,
    options: &OpenOptions,
) -> LinuxResult<c_int>
where
    E: Into<LinuxError>,
    F: FnOnce(&str, &OpenOptions) -> Result<axfs::fops::File, E>,
    D: FnOnce(&str, &OpenOptions) -> Result<axfs::fops::Directory, E>,
{
    open_file(filename, options)
        .map_err(Into::into)
        .map(File::new)
        .and_then(File::add_to_fd_table)
        .or_else(|e| {
            match e {
                // LinuxError::EINVAL
                _ => {
                    let mut options = options.clone();
                    options.execute(true);
                    options.create_new(false);
                    open_dir(filename, &options)
                        .map_err(Into::into)
                        .map(|d| Directory::new(d, filename.into()))
                        .and_then(Directory::add_to_fd_table)
                } // _ => Err(e.into()),
            }
        })
}

/// Set the position of the file indicated by `fd`.
///
/// Return its position after seek.
pub fn sys_lseek(fd: c_int, offset: ctypes::off_t, whence: c_int) -> ctypes::off_t {
    debug!("sys_lseek <= {} {} {}", fd, offset, whence);
    syscall_body!(sys_lseek, {
        let pos = match whence {
            0 => SeekFrom::Start(offset as _),
            1 => SeekFrom::Current(offset as _),
            2 => SeekFrom::End(offset as _),
            _ => return Err(LinuxError::EINVAL),
        };
        let off = File::from_fd(fd)?.inner.lock().seek(pos)?;
        Ok(off)
    })
}

/// Get the file metadata by `path` and write into `buf`.
///
/// Return 0 if success.
pub unsafe fn sys_stat(path: *const c_char, buf: *mut ctypes::stat) -> c_int {
    let path = char_ptr_to_str(path);
    debug!("sys_stat <= {:?} {:#x}", path, buf as usize);
    syscall_body!(sys_stat, {
        if buf.is_null() {
            return Err(LinuxError::EFAULT);
        }
        let mut options = OpenOptions::new();
        options.read(true);
        let file = axfs::fops::File::open(path?, &options)?;
        let st = File::new(file).stat()?;
        unsafe { *buf = st };
        Ok(0)
    })
}

/// Get file metadata by `fd` and write into `buf`.
///
/// Return 0 if success.
pub unsafe fn sys_fstat(fd: c_int, buf: *mut ctypes::stat) -> c_int {
    debug!("sys_fstat <= {} {:#x}", fd, buf as usize);
    syscall_body!(sys_fstat, {
        if buf.is_null() {
            return Err(LinuxError::EFAULT);
        }

        unsafe { *buf = get_file_like(fd)?.stat()? };
        Ok(0)
    })
}

/// Get the metadata of the symbolic link and write into `buf`.
///
/// Return 0 if success.
pub unsafe fn sys_lstat(path: *const c_char, buf: *mut ctypes::stat) -> ctypes::ssize_t {
    let path = char_ptr_to_str(path);
    debug!("sys_lstat <= {:?} {:#x}", path, buf as usize);
    syscall_body!(sys_lstat, {
        if buf.is_null() {
            return Err(LinuxError::EFAULT);
        }
        unsafe { *buf = Default::default() }; // TODO
        Ok(0)
    })
}

pub fn sys_mkdirat(dirfd: c_int, pathname: *const c_char, mode: ctypes::mode_t) -> c_int {
    let pathname = char_ptr_to_str(pathname);
    debug!("sys_mkdirat <= {} {:?} {:#o}", dirfd, pathname, mode);
    syscall_body!(sys_mkdirat, {
        let pathname = pathname?;
        if pathname.starts_with('/') || dirfd == -100 {
            return axfs::api::create_dir(pathname)
                .map(|_| 0)
                .map_err(Into::into);
        }

        let dir = Directory::from_fd(dirfd)?;
        dir.inner.lock().create_dir(pathname)?;
        Ok(0)
    })
}

pub fn sys_chdir(path: *const c_char) -> c_int {
    syscall_body!(sys_chdir, {
        let path = char_ptr_to_str(path)?;
        debug!("sys_chdir <= {:?}", path);
        axfs::api::set_current_dir(path)?;
        Ok(0)
    })
}

/// Get the path of the current directory.
pub fn sys_getcwd(buf: *mut c_char, size: usize) -> *mut c_char {
    debug!("sys_getcwd <= {:#x} {}", buf as usize, size);
    syscall_body!(sys_getcwd, {
        if buf.is_null() {
            return Ok(core::ptr::null::<c_char>() as _);
        }
        let dst = unsafe { core::slice::from_raw_parts_mut(buf as *mut u8, size as _) };
        let cwd = axfs::api::current_dir()?;
        let cwd = cwd.as_bytes();
        if cwd.len() < size {
            dst[..cwd.len()].copy_from_slice(cwd);
            dst[cwd.len()] = 0;
            Ok(buf)
        } else {
            Err(LinuxError::ERANGE)
        }
    })
}

/// Rename `old` to `new`
/// If new exists, it is first removed.
///
/// Return 0 if the operation succeeds, otherwise return -1.
pub fn sys_rename(old: *const c_char, new: *const c_char) -> c_int {
    syscall_body!(sys_rename, {
        let old_path = char_ptr_to_str(old)?;
        let new_path = char_ptr_to_str(new)?;
        debug!("sys_rename <= old: {:?}, new: {:?}", old_path, new_path);
        axfs::api::rename(old_path, new_path)?;
        Ok(0)
    })
}

/// FAT file system does not support `linkat` syscall.
/// So unlinkat is just a wrapper of `remove_file`.
pub fn sys_unlinkat(dirfd: i32, pathname: *const c_char, flags: i32) -> i32 {
    let pathname = char_ptr_to_str(pathname);
    debug!("unlinkat <= {} {:?} {:#x}", dirfd, pathname, flags);
    syscall_body!(unlinkat, {
        let pathname = pathname?;
        if pathname.starts_with('/') || dirfd == -100 {
            return axfs::api::remove_file(pathname)
                .map(|_| 0)
                .map_err(Into::into);
        }

        let dir = Directory::from_fd(dirfd)?;
        dir.inner.lock().remove_file(pathname)?;
        Ok(0)
    })
}

pub fn sys_mount(
    source: *const c_char,
    target: *const c_char,
    fstype: *const c_char,
    flags: u64,
    data: *const c_void,
) -> i32 {
    axfs::api::mount(source, target, fstype, flags, data)
}

pub fn sys_umount(target: *const c_char) -> i32 {
    axfs::api::unmount(target)
}
