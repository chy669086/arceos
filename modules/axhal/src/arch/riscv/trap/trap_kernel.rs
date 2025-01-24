use linkme::distributed_slice;

#[distributed_slice]
pub static INTO_KERNEL: [fn()];

#[distributed_slice]
pub static INTO_USER: [fn()];

pub(crate) fn into_kernel() {
    INTO_KERNEL[0]();
}

pub(crate) fn into_user() {
    INTO_USER[0]();
}