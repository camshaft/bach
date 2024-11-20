use super::*;
use crate::sync::queue::Queue as _;

/// Overflow behavior should be the same regardless of discipline
macro_rules! overflow_tests {
    () => {
        #[test]
        fn prefer_oldest() {
            let queue = Queue::builder()
                .with_capacity(Some(2))
                .with_discipline(DISCIPLINE)
                .with_overflow(Overflow::PreferOldest)
                .build();

            queue.push(0).unwrap();
            queue.push(1).unwrap();
            assert!(matches!(queue.push(2), Err(PushError::Full(2))));
        }

        #[test]
        fn prefer_recent() {
            let queue = Queue::builder()
                .with_capacity(Some(2))
                .with_discipline(DISCIPLINE)
                .with_overflow(Overflow::PreferRecent)
                .build();

            queue.push(0).unwrap();
            queue.push(1).unwrap();
            assert!(matches!(queue.push(2), Ok(Some(0))));
        }
    };
}

macro_rules! push_pop_test {
    ([$($value:expr),* $(,)?]) => {
        #[test]
        fn push_pop() {
            let queue = Queue::builder()
                .with_discipline(DISCIPLINE)
                .build();

            let mut v = 0;

            $(
                queue.push(v).unwrap();
                v += 1;
                let _ = $value;
            )*

            let _ = v;

            $(
                assert_eq!(queue.pop().unwrap(), $value);
            )*
        }
    };
}

mod lifo {
    use super::*;
    const DISCIPLINE: Discipline = Discipline::Lifo;
    overflow_tests!();

    push_pop_test!([5, 4, 3, 2, 1, 0]);
}

mod fifo {
    use super::*;
    const DISCIPLINE: Discipline = Discipline::Fifo;
    overflow_tests!();

    push_pop_test!([0, 1, 2, 3, 4, 5]);
}
