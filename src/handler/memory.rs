#[allow(clippy::missing_const_for_fn)]
#[cfg(target_os = "linux")]
#[inline(always)]
pub fn log_memory(prefix: &'static str) {
    use log::info;
    use procinfo;
    info!("{prefix}: {:?}", procinfo::pid::statm_self());
}

#[allow(clippy::missing_const_for_fn)]
#[cfg(not(target_os = "linux"))]
#[inline(always)]
pub fn log_memory(_prefix: &'static str) {}
