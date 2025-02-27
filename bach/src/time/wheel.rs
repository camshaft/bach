use super::{
    entry::{Entry, Queue},
    stack::Stack,
};

#[derive(Debug)]
pub struct Wheel<E: Entry> {
    stacks: [Stack<E>; 8],
    pending_wake: E::Queue,
}

impl<E: Entry> Default for Wheel<E> {
    fn default() -> Self {
        Self {
            stacks: Default::default(),
            pending_wake: E::Queue::new(),
        }
    }
}

macro_rules! stack_map {
    ($stack:expr, $cb:expr) => {{
        let cb = $cb;
        [
            cb(&$stack[0]),
            cb(&$stack[1]),
            cb(&$stack[2]),
            cb(&$stack[3]),
            cb(&$stack[4]),
            cb(&$stack[5]),
            cb(&$stack[6]),
            cb(&$stack[7]),
        ]
    }};
}

impl<E: Entry> Wheel<E> {
    pub fn ticks(&self) -> u64 {
        u64::from_le_bytes(stack_map!(self.stacks, |stack: &Stack<E>| stack.current()))
    }

    pub fn is_empty(&self) -> bool {
        macro_rules! is_empty {
            () => {
                is_empty!([0, 1, 2, 3, 4, 5, 6, 7])
            };
            ([$($idx:expr),*]) => {
                $(self.stacks[$idx].is_empty())&*
            };
        }
        is_empty!()
    }

    pub fn insert(&mut self, mut entry: E) {
        let ticks = self.ticks();
        entry.set_start_tick(ticks);
        self.insert_at(entry, ticks);
    }

    fn insert_at(&mut self, entry: E, start_tick: u64) -> bool {
        let delay = entry.delay();
        let absolute_time = delay.wrapping_add(start_tick);
        let now = self.ticks();
        let zero_time = (absolute_time ^ now).to_be();

        if zero_time == 0 {
            self.pending_wake.push(entry);
            return true;
        }

        let absolute_bytes = absolute_time.to_le_bytes();
        let leading = zero_time.leading_zeros();

        let index = (leading / 8) as usize;
        let position = absolute_bytes[index];

        self.stack_mut(index).insert(position, entry);

        false
    }

    pub fn advance(&mut self) -> Option<u64> {
        let start = self.ticks();
        let has_pending = !self.pending_wake.is_empty();

        if has_pending {
            return Some(0);
        }

        if self.is_empty() {
            return None;
        }

        let mut iterations = 0;

        while !self.advance_once()? {
            debug_assert!(iterations < u16::MAX, "advance iterated too many times");
            iterations += 1;
        }

        // TODO handle wheel wrapping

        Some(self.ticks() - start)
    }

    fn advance_once(&mut self) -> Option<bool> {
        let mut can_skip = true;
        let mut is_empty = true;
        let mut has_pending = false;

        for index in 0..self.stacks.len() {
            let (mut list, did_wrap) = self.stack_mut(index).tick(can_skip);

            while let Some(entry) = list.pop() {
                let start_tick = entry.start_tick();
                if self.insert_at(entry, start_tick) {
                    // A pending item is ready
                    has_pending = true;
                } else {
                    // the item was pushed above the current stack so
                    // we can't skip anymore
                    can_skip = false;
                }

                // in either case we know there's some available entry
                is_empty = false;
            }

            // we can only proceed to the next stack if the current wrapped
            if !did_wrap {
                return Some(has_pending);
            }

            // children can only skip if this is also empty
            can_skip &= self.stack_mut(index).is_empty();
            is_empty &= can_skip;
        }

        if is_empty {
            return None;
        }

        Some(has_pending)
    }

    #[inline]
    pub fn wake<F: FnMut(E)>(&mut self, mut wake: F) -> usize {
        let mut count = 0;

        let mut pending = self.pending_wake.take();

        while let Some(entry) = pending.pop() {
            count += 1;
            wake(entry);
        }

        count
    }

    #[inline]
    pub fn close<F: FnMut(E)>(&mut self, mut close: F) {
        if self.is_empty() {
            return;
        }

        while let Some(entry) = self.pending_wake.pop() {
            close(entry);
        }

        for stack in self.stacks.iter_mut() {
            stack.close(&mut close);
        }
    }

    #[inline]
    pub fn reset(&mut self) {
        for stack in self.stacks.iter_mut() {
            stack.reset();
        }
    }

    #[inline]
    fn stack_mut(&mut self, index: usize) -> &mut Stack<E> {
        if cfg!(test) {
            debug_assert!(index < self.stacks.len());
        }
        unsafe { self.stacks.get_unchecked_mut(index) }
    }
}

#[cfg(test)]
mod tests {
    use super::{super::entry::atomic, *};
    use alloc::{vec, vec::Vec};
    use bolero::{check, generator::*};
    use core::time::Duration;

    #[test]
    fn insert_advance_wake_check() {
        let max_ticks = Duration::from_secs(100_000).as_nanos() as u64;

        let entry = produce::<Vec<u64>>().with().values(0..max_ticks);
        let entries = produce::<Vec<_>>().with().values(entry);

        check!().with_generator(entries).for_each(|entries| {
            test_helper(&entries[..]);
        });
    }

    fn test_helper<T: AsRef<[u64]>>(entries: &[T]) {
        let mut wheel = Wheel::default();
        let mut sorted = vec![];

        let mut total_ticks = 0;

        for entries in entries.iter().map(AsRef::as_ref) {
            sorted.extend_from_slice(entries);
            sorted.sort_unstable();

            let mut should_wake = false;
            for entry in entries.iter().copied() {
                // adding a 0-tick will immediately wake the entry
                should_wake |= entry == 0;
                wheel.insert(atomic::Entry::new(entry));
            }

            let mut sorted = sorted.drain(..);

            let woken = wheel.wake(atomic::wake);

            assert_eq!(woken > 0, should_wake);

            for _ in 0..woken {
                sorted.next();
            }

            let mut elapsed = 0;

            while let Some(expected) = sorted.next() {
                let delta = expected - elapsed;
                assert_eq!(wheel.advance(), Some(delta));
                elapsed += delta;

                assert_eq!(
                    wheel.advance(),
                    Some(0),
                    "the wheel should not advance while there are pending items"
                );

                for _ in (0..wheel.wake(atomic::wake)).skip(1) {
                    assert_eq!(
                        sorted.next(),
                        Some(expected),
                        "any additional items should be equal"
                    );
                }
            }

            assert!(wheel.is_empty());
            assert_eq!(wheel.advance(), None);
            assert_eq!(wheel.wake(atomic::wake), 0);
            assert!(wheel.is_empty());

            total_ticks += elapsed;

            assert_eq!(wheel.ticks(), total_ticks);
        }
    }

    #[test]
    fn empty_test() {
        let mut wheel = Wheel::default();
        assert_eq!(wheel.ticks(), 0);
        assert!(wheel.is_empty());
        assert_eq!(wheel.advance(), None);
        assert_eq!(wheel.wake(atomic::wake), 0);
    }

    #[test]
    fn crossing_test() {
        for t in [250..260, 510..520, 65790..65800]
            .iter()
            .flat_map(|v| v.clone())
        {
            test_helper(&[[t, t + 1]]);
        }
    }

    #[test]
    fn duplicate_test() {
        test_helper(&[&[1, 489][..], &[24, 279][..]]);
    }

    #[test]
    fn overflow_test() {
        test_helper(&[
            &[3588254211306][..],
            &[799215800378, 10940666347][..],
            &[][..],
        ]);
    }
}
