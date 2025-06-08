use super::*;
use crate::testing::Log;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Event {
    Start,
    MutexAcquired { task: usize },
    MutexReleased { task: usize },
}

impl crate::testing::Event for Event {
    fn is_start(&self) -> bool {
        matches!(self, Event::Start)
    }
}

#[test]
fn mutex_try_lock() {
    bolero::check!().exhaustive().run(sim(|| {
        async {
            let mutex = Mutex::new(5);

            // Successfully try_lock
            let mut guard = mutex.try_lock().unwrap();
            *guard = 10;

            // This should fail while the first lock is held
            assert!(mutex.try_lock().is_err());

            // Release the first lock
            drop(guard);

            // Now try_lock should succeed
            let guard = mutex.try_lock().unwrap();
            assert_eq!(*guard, 10);
        }
        .primary()
        .spawn();
    }));
}

#[test]
fn mutex_guard_deref() {
    bolero::check!().exhaustive().run(sim(|| {
        async {
            let mutex = Mutex::new(String::from("hello"));

            // Test that we can access the value through the guard
            let guard = mutex.lock().await;
            assert_eq!(guard.len(), 5);
            assert_eq!(*guard, "hello");
            drop(guard);

            // Test that we can mutate the value through the guard
            let mut guard = mutex.lock().await;
            guard.push_str(" world");
            assert_eq!(*guard, "hello world");
        }
        .primary()
        .spawn();
    }));
}

#[test]
fn mutex_lock_contention() {
    // For tracking test events
    static LOG: Log<Event> = Log::new();

    bolero::check!().exhaustive().run(sim(|| {
        let mutex = Arc::new(Mutex::new(0));

        // Start a task that holds the lock for a while
        {
            let mutex = mutex.clone();
            async move {
                let mut guard = mutex.lock().await;
                *guard = 1;
                LOG.push(Event::MutexAcquired { task: 0 });

                // Hold the lock for some time
                bach::time::sleep(std::time::Duration::from_millis(20)).await;
                *guard = 2;

                drop(guard);
                LOG.push(Event::MutexReleased { task: 0 });
            }
            .primary()
            .spawn();
        }

        // Try to acquire a different mutex to avoid clone issues
        async move {
            // Acquire our own mutex
            let guard = mutex.lock().await;
            LOG.push(Event::MutexAcquired { task: 1 });

            drop(guard);
            LOG.push(Event::MutexReleased { task: 1 });
        }
        .primary()
        .spawn();
    }));

    insta::assert_debug_snapshot!(LOG.check());
}

#[test]
fn mutex_interleavings_snapshot() {
    static LOG: Log<Event> = Log::new();

    bolero::check!().exhaustive().run(sim(|| {
        // Start a new event sequence
        LOG.push(Event::Start);

        // Create a mutex
        let mutex = Arc::new(Mutex::new(0));

        // Launch multiple tasks that will contend for the mutex
        for task_id in 0..3 {
            let mutex = mutex.clone();
            async move {
                // Acquire the mutex
                let mut value = mutex.lock().await;
                LOG.push(Event::MutexAcquired { task: task_id });

                // Do some work with the mutex
                *value += 1;

                // Release the mutex (implicit when guard is dropped)
                drop(value);
                LOG.push(Event::MutexReleased { task: task_id });
            }
            .primary()
            .spawn_named(format!("Task {task_id}"));
        }
    }));

    insta::assert_debug_snapshot!(LOG.check());
}

#[test]
fn mutex_owned_lock() {
    bolero::check!().exhaustive().run(sim(|| {
        async {
            // Create an Arc-wrapped mutex
            let mutex = Arc::new(Mutex::new(5));

            // Use owned_lock to get an owned guard
            let mut guard = mutex.clone().lock_owned().await;

            // Test that we can access and modify the value through the guard
            assert_eq!(*guard, 5);
            *guard = 10;

            // Spawn a task that holds the owned guard across an await point
            async move {
                // Wait on the channel while holding the guard
                10.ms().sleep().await;

                // Verify the guard still works after the await
                assert_eq!(*guard, 10);
                *guard = 15;

                // Return the guard to verify the value later
                guard
            }
            .spawn();

            // Wait for the task to complete and get the guard back
            let guard = mutex.lock().await;

            // Verify the value was changed in the task
            assert_eq!(*guard, 15);
        }
        .primary()
        .spawn();
    }));
}

#[test]
#[should_panic = "Runtime stalled"]
fn mutex_deadlock_detection() {
    // Set up a scenario with two tasks and two mutexes where a deadlock can occur
    sim(|| {
        // Create two mutexes
        let mutex1 = Arc::new(Mutex::new(0));
        let mutex2 = Arc::new(Mutex::new(0));

        // Task 1: acquire mutex1, then try to acquire mutex2
        {
            let mutex1 = mutex1.clone();
            let mutex2 = mutex2.clone();
            async move {
                // Acquire the first mutex
                let guard1 = mutex1.lock().await;

                // Try to acquire the second mutex - this should lead to a deadlock
                // if task 2 is already holding mutex2
                let _guard2 = mutex2.lock().await;

                // Release both mutexes
                drop(_guard2);
                drop(guard1);
            }
            .primary()
            .spawn_named("Task 1");
        }

        // Task 2: acquire mutex2, then try to acquire mutex1
        {
            let mutex1 = mutex1.clone();
            let mutex2 = mutex2.clone();
            async move {
                // Acquire the second mutex
                let guard2 = mutex2.lock().await;

                // Try to acquire the first mutex - this should lead to a deadlock
                // if task 1 is already holding mutex1
                let _guard1 = mutex1.lock().await;

                // Release both mutexes
                drop(_guard1);
                drop(guard2);
            }
            .primary()
            .spawn_named("Task 2");
        }
    })();
}
