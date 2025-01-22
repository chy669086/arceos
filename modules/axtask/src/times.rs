use crate::current;
use cfg_if::cfg_if;
use linkme::distributed_slice;

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct Tms {
    pub tms_utime: isize,
    pub tms_stime: isize,
    pub tms_cutime: isize,
    pub tms_cstime: isize,
}

impl Tms {
    pub fn new_empty() -> Self {
        Tms {
            tms_utime: 0,
            tms_stime: 0,
            tms_cutime: 0,
            tms_cstime: 0,
        }
    }

    pub fn create_from_times(process: &Times, children: &Times) -> Self {
        Tms {
            tms_utime: process.utime,
            tms_stime: process.stime,
            tms_cutime: children.utime,
            tms_cstime: children.stime,
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum TimesType {
    Kernel(isize),
    User(isize),
    None,
}

/// 一个辅助结构体，用于记录进程的用户态和内核态时间
#[derive(Debug, Clone, Copy)]
pub struct Times {
    start_time: TimesType,
    pub(crate) utime: isize,
    pub(crate) stime: isize,
}

impl Times {
    pub fn new() -> Self {
        Times {
            start_time: TimesType::User(-1),
            utime: 0,
            stime: 0,
        }
    }

    pub fn add(&mut self, other: &Self) {
        self.utime += other.utime;
        self.stime += other.stime;
    }

    /// 设置 `start_time` 为当前时间
    pub fn set_curr_time(&mut self, is_kernel: bool) {
        let cur_time = axhal::time::current_ticks();
        self.set_start_time(cur_time as isize, is_kernel);
    }

    pub fn reset_time(&mut self) {
        match self.start_time {
            TimesType::Kernel(ref mut time) => {
                *time = axhal::time::current_ticks() as isize;
            }
            TimesType::User(ref mut time) => {
                *time = axhal::time::current_ticks() as isize;
            }
            _ => (),
        }
    }

    pub fn set_start_time(&mut self, cur_time: isize, is_kernel: bool) {
        if is_kernel {
            self.start_time = TimesType::Kernel(cur_time);
        } else {
            self.start_time = TimesType::User(cur_time);
        }
    }

    pub fn update_time_by_curr(&mut self) {
        let cur_time = axhal::time::current_ticks() as isize;
        self.update_time(cur_time);
    }

    /// 更新时间，在更新时间之后，将start_time设置为-1
    pub fn update_time(&mut self, cur_time: isize) {
        if !self.is_valid() {
            warn!("Current time is not valid");
            return;
        }
        match self.start_time {
            TimesType::Kernel(start_time) => {
                self.stime = cur_time - start_time;
            }
            TimesType::User(start_time) => {
                self.utime = cur_time - start_time;
            }
            TimesType::None => {
                warn!("Times::update_time: start_time is None!");
            }
        }
        self.change_to_wait();
    }

    fn change_to_wait(&mut self) {
        match self.start_time {
            TimesType::Kernel(ref mut time) => *time = -1,
            TimesType::User(ref mut time) => *time = -1,
            _ => (),
        }
    }

    fn is_valid(&self) -> bool {
        match self.start_time {
            TimesType::Kernel(-1) | TimesType::User(-1) | TimesType::None => false,
            _ => true,
        }
    }
}

cfg_if! {
    if #[cfg(feature = "multitask")] {
        use axhal::arch::{INTO_KERNEL, INTO_USER};

        #[distributed_slice(INTO_KERNEL)]
        fn into_kernel() {
            let curr = current();
            curr.update_time();
            curr.set_start_time(true);
        }

        #[distributed_slice(INTO_USER)]
        fn into_user() {
            let curr = current();
            curr.update_time();
            curr.set_start_time(false);
        }
    }
}
