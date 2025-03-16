use super::*;
use crate::queue::Queue as _;

/// Overflow behavior should be the same regardless of discipline
macro_rules! overflow_tests {
    () => {
        #[test]
        fn prefer_oldest() {
            let mut queue = Queue::builder()
                .with_capacity(Some(2))
                .with_discipline(DISCIPLINE)
                .with_overflow(Overflow::PreferOldest)
                .build();

            queue.push(0).unwrap();
            queue.push(1).unwrap();
            assert!(matches!(queue.push_lazy(&mut None), Err(PushError::Full)));
        }

        #[test]
        fn prefer_recent() {
            let mut queue = Queue::builder()
                .with_capacity(Some(2))
                .with_discipline(DISCIPLINE)
                .with_overflow(Overflow::PreferRecent)
                .build();

            queue.push(0).unwrap();
            queue.push(1).unwrap();
            let mut v = Some(2);
            assert!(matches!(queue.push_lazy(&mut v), Ok(Some(0))));
            assert_eq!(v, None);
        }
    };
}

macro_rules! push_pop_test {
    ([$($value:expr),* $(,)?]) => {
        #[test]
        fn push_pop() {
            let mut queue = Queue::builder()
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
