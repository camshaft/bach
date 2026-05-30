/// Called before polling a non-internal task.
///
/// When the `valgrind` feature is enabled and the binary is running under
/// Callgrind with `--collect-at-start=no`, this turns instrumentation ON so that
/// only the user task's instructions are counted.
#[inline(always)]
pub fn task_poll_begin() {
    #[cfg(feature = "valgrind")]
    crabgrind::callgrind::start_instrumentation();
}

/// Called after polling a non-internal task.
///
/// Turns Callgrind instrumentation back OFF so that bach's scheduler machinery
/// is excluded from the profile.
#[inline(always)]
pub fn task_poll_end() {
    #[cfg(feature = "valgrind")]
    crabgrind::callgrind::stop_instrumentation();
}

pub struct TaskPollGuard(());

impl TaskPollGuard {
    #[inline(always)]
    pub fn new() -> Self {
        task_poll_begin();
        Self(())
    }
}

impl Drop for TaskPollGuard {
    #[inline(always)]
    fn drop(&mut self) {
        task_poll_end();
    }
}
