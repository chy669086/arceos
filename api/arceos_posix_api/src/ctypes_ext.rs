use crate::ctypes::timespec;

pub const UTIME_NOW: core::ffi::c_long = 0x3FFFFFFF;
pub const UTIME_OMIT: core::ffi::c_long = 0x3FFFFFFE;

impl timespec {
    pub fn now() -> Self {
        axhal::time::wall_time().into()
    }

    pub fn set_as_utime(&mut self, time: Self) {
        match time.tv_nsec {
            UTIME_NOW => {
                *self = timespec::now();
            }
            UTIME_OMIT => {}
            _ => *self = time,
        }
    }
}
