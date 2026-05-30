/// Called before polling a non-internal task.
///
/// When the `valgrind` feature is enabled and the binary is running under
/// Callgrind with `--collect-at-start=no`, this toggles collection ON so that
/// only the user task's instructions are counted.
#[inline(always)]
pub fn task_poll_begin() {
    #[cfg(feature = "valgrind")]
    crabgrind::callgrind::toggle_collect();
}

/// Called after polling a non-internal task.
///
/// Toggles Callgrind collection back OFF so that bach's scheduler machinery
/// is excluded from the profile.
#[inline(always)]
pub fn task_poll_end() {
    #[cfg(feature = "valgrind")]
    crabgrind::callgrind::toggle_collect();
}
