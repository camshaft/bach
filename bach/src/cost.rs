use core::time::Duration;

pub fn record(cost: Duration) {
    crate::task::info::scope::try_borrow_with(|scope| {
        if let Some(scope) = scope.as_ref() {
            scope.record_cost(cost);
        }
    });
}

pub(crate) fn debt() -> u64 {
    crate::task::info::scope::try_borrow_with(|scope| scope.as_ref().map(|s| s.debt())).unwrap_or(0)
}
