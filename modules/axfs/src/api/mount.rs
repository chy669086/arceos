use alloc::boxed::Box;
use alloc::format;
use alloc::string::{String, ToString};
use axerrno::AxResult;
use core::ffi::{c_char, c_void, CStr};

pub fn mount(
    source: *const c_char,
    target: *const c_char,
    fstype: *const c_char,
    _flags: u64,
    _data: *const c_void,
) -> i32 {
    let source = unsafe { CStr::from_ptr(source).to_str().unwrap() };
    let target = unsafe { CStr::from_ptr(target).to_str().unwrap() };
    let fstype = unsafe { CStr::from_ptr(fstype).to_str().unwrap() };
    let file_system = match crate::root::find_mounted_fs(source) {
        Ok(fs) => fs,
        Err(e) => {
            warn!("mount: find_mounted_fs failed: {:?}", e);
            return -1;
        }
    };

    let target = to_root_path(target).unwrap();

    let target = Box::leak(target.into_boxed_str());

    match crate::root::mount_fs(target, file_system) {
        Ok(()) => {}
        Err(e) => {
            warn!("mount: mount_fs failed: {:?}", e);
            return -1;
        }
    }
    0
}

pub fn unmount(target: *const c_char) -> i32 {
    let target = unsafe { CStr::from_ptr(target).to_str().unwrap() };
    let target = to_root_path(target).unwrap();
    crate::root::unmount_fs(&target);
    0
}

fn to_root_path(path: &str) -> AxResult<String> {
    if path.is_empty() {
        return Err(axerrno::AxError::InvalidInput);
    }
    if path.starts_with('/') {
        return Ok(path.to_string());
    }
    let path = if let Some(path) = path.strip_prefix("./") {
        path
    } else {
        path
    };
    let cwd = crate::root::current_dir()?;
    Ok(format!("{}/{}", cwd, path))
}