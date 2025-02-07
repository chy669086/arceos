use crate::fops::File;
use alloc::format;
use alloc::string::{String, ToString};
use cfg_if::cfg_if;
use core::ffi::{c_char, c_int, c_void, CStr};

/// utimensat - change file timestamps with nanosecond precision
pub fn utimensat(
    pathname: *const c_char,
    atime: u32,
    mtime: u32,
    flag: c_int,
) -> axerrno::LinuxResult<c_int> {
    cfg_if! {
        if #[cfg(feature = "myfs")] {
            todo!()
        } else if #[cfg(feature = "lwext4_rust")] {
            return lwext4_rust_utimensat(pathname, atime, mtime, flag);
        } else {
            todo!()
        }
    }
}

pub fn get_file_utime(file: &File) -> (u32, u32) {
    cfg_if! {
        if #[cfg(feature = "myfs")] {
            todo!()
        } else if #[cfg(feature = "lwext4_rust")] {
            return lwext4_rust_get_file_utime(file);
        } else {
            todo!()
        }
    }
}

pub fn get_file_path(file: &File) -> Option<String> {
    cfg_if! {
        if #[cfg(feature = "myfs")] {
            todo!()
        } else if #[cfg(feature = "lwext4_rust")] {
            return lwext4_rust_get_file_path(file);
        } else {
            todo!()
        }
    }
}

#[cfg(feature = "lwext4_rust")]
fn lwext4_rust_get_file_path(file: &File) -> Option<String> {
    unsafe { file.node.access_unchecked() }
        .as_any()
        .downcast_ref::<crate::fs::lwext4_rust::FileWrapper>()
        .map(|f| f.get_path())
}

#[cfg(feature = "lwext4_rust")]
fn lwext4_rust_get_file_utime(file: &File) -> (u32, u32) {
    let file = unsafe { file.node.access_unchecked() }
        .as_any()
        .downcast_ref::<lwext4_rust::Ext4File>()
        .unwrap();
    let path = file.get_path();
    let path = path.as_c_str().as_ptr();
    let (mut atime, mut mtime) = (0u32, 0u32);
    unsafe {
        use lwext4_rust::bindings::*;
        let _ = ext4_atime_get(path, &mut atime as *mut _);
        let _ = ext4_mtime_get(path, &mut mtime as *mut _);
        (atime, mtime)
    }
}

#[cfg(feature = "lwext4_rust")]
fn lwext4_rust_utimensat(
    pathname: *const c_char,
    atime: u32,
    mtime: u32,
    _flag: c_int,
) -> axerrno::LinuxResult<c_int> {
    unsafe {
        use lwext4_rust::bindings::*;
        ext4_atime_set(pathname, atime);
        ext4_mtime_set(pathname, mtime);
    }
    Ok(0)
}
