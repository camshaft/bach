use super::*;
use crate::testing::Log;
use bach::sync::RwLock;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Event {
    Start,
    ReadAcquired { task: usize },
    ReadReleased { task: usize },
    WriteAcquired { task: usize },
    WriteReleased { task: usize },
}

impl crate::testing::Event for Event {
    fn is_start(&self) -> bool {
        matches!(self, Self::Start)
    }
}

#[test]
fn rwlock_readers() {
    bolero::check!().exhaustive().run(sim(|| {
        async {
            let rwlock = RwLock::new(5);

            // Test multiple readers can access concurrently
            let read1 = rwlock.read().await;
            assert_eq!(*read1, 5);

            // Second reader can acquire while first reader holds the lock
            let read2 = rwlock.read().await;
            assert_eq!(*read2, 5);

            // Release first reader
            drop(read1);

            // Value should still be accessible through second reader
            assert_eq!(*read2, 5);

            // Release second reader
            drop(read2);
        }
        .primary()
        .spawn();
    }));
}

#[test]
fn rwlock_try_read() {
    bolero::check!().exhaustive().run(sim(|| {
        async {
            let rwlock = RwLock::new(10);

            // Successfully try_read
            let read1 = rwlock.try_read().unwrap();
            assert_eq!(*read1, 10);

            // Multiple readers should succeed
            let read2 = rwlock.try_read().unwrap();
            assert_eq!(*read2, 10);

            // Clean up
            drop(read1);
            drop(read2);
        }
        .primary()
        .spawn();
    }));
}

#[test]
fn rwlock_writer() {
    bolero::check!().exhaustive().run(sim(|| {
        async {
            let rwlock = RwLock::new(5);

            // Acquire write lock
            let mut write = rwlock.write().await;

            // Modify the value
            *write = 10;
            assert_eq!(*write, 10);

            // Release write lock
            drop(write);

            // Confirm value was changed
            let read = rwlock.read().await;
            assert_eq!(*read, 10);
        }
        .primary()
        .spawn();
    }));
}

#[test]
fn rwlock_try_write() {
    bolero::check!().exhaustive().run(sim(|| {
        async {
            let rwlock = RwLock::new(5);

            // Successfully try_write
            let mut write = rwlock.try_write().unwrap();
            *write = 15;
            assert_eq!(*write, 15);

            // Clean up
            drop(write);
        }
        .primary()
        .spawn();
    }));
}

#[test]
fn rwlock_read_write_exclusion() {
    bolero::check!().exhaustive().run(sim(|| {
        async {
            let rwlock = RwLock::new(5);

            // Acquire read lock
            let read = rwlock.read().await;

            // Try to write should fail while reading
            assert!(rwlock.try_write().is_err());

            // Release read lock
            drop(read);

            // Now write should succeed
            let mut write = rwlock.try_write().unwrap();
            *write = 20;

            // Try to read should fail while writing
            assert!(rwlock.try_read().is_err());

            // Release write lock
            drop(write);

            // Now read should succeed
            let read = rwlock.try_read().unwrap();
            assert_eq!(*read, 20);
        }
        .primary()
        .spawn();
    }));
}

#[test]
fn rwlock_multiple_tasks() {
    static LOG: Log<Event> = Log::new();

    bolero::check!().exhaustive().run(sim(|| {
        // Start a new event sequence
        LOG.push(Event::Start);

        let rwlock = Arc::new(RwLock::new(0));

        // Multiple reader tasks
        for i in 0..3 {
            let rwlock = rwlock.clone();
            async move {
                let read = rwlock.read().await;
                LOG.push(Event::ReadAcquired { task: i }); // Log that reader acquired lock
                bach::time::sleep(std::time::Duration::from_millis(5)).await;
                drop(read);
                LOG.push(Event::ReadReleased { task: i });
            }
            .primary()
            .spawn_named(format!("Reader {i}"));
        }

        // Writer task
        {
            let rwlock = rwlock.clone();
            async move {
                let mut write = rwlock.write().await;
                LOG.push(Event::WriteAcquired { task: 0 }); // Writer acquired lock
                *write = 50;
                bach::time::sleep(std::time::Duration::from_millis(5)).await;
                drop(write);
            }
            .primary()
            .spawn_named("Writer");
        }

        // Final reader to verify the updated value
        {
            let rwlock = rwlock.clone();
            async move {
                bach::time::sleep(std::time::Duration::from_millis(50)).await;

                let read = rwlock.read().await;
                assert_eq!(*read, 50);
            }
            .primary()
            .spawn_named("Final reader");
        }
    }));

    insta::assert_debug_snapshot!(LOG.check());
}

#[test]
fn rwlock_interleavings_snapshot() {
    static LOG: Log<Event> = Log::new();

    bolero::check!().exhaustive().run(sim(|| {
        // Start a new event sequence
        LOG.push(Event::Start);

        // Create a rwlock
        let rwlock = Arc::new(RwLock::new(0));

        // Launch two readers
        for task_id in 0..2 {
            let rwlock = rwlock.clone();
            async move {
                // Acquire read lock
                let read = rwlock.read().await;
                LOG.push(Event::ReadAcquired { task: task_id });

                // Hold the lock briefly
                bach::time::sleep(std::time::Duration::from_millis(5)).await;

                // Release the read lock
                drop(read);
                LOG.push(Event::ReadReleased { task: task_id });
            }
            .primary()
            .spawn_named(format!("Reader {task_id}"));
        }

        // Launch a writer
        {
            let rwlock = rwlock.clone();
            async move {
                // Acquire write lock
                let mut write = rwlock.write().await;
                LOG.push(Event::WriteAcquired { task: 2 });

                // Modify the value
                *write += 1;
                bach::time::sleep(std::time::Duration::from_millis(5)).await;

                // Release the write lock
                drop(write);
                LOG.push(Event::WriteReleased { task: 2 });
            }
            .primary()
            .spawn_named("Writer");
        }
    }));

    insta::assert_debug_snapshot!(LOG.check());
}

#[test]
#[should_panic = "Runtime stalled"]
fn rwlock_deadlock_detection() {
    sim(|| {
        // Create two rwlocks
        let rwlock1 = Arc::new(RwLock::new(0));
        let rwlock2 = Arc::new(RwLock::new(0));

        // Task 1: acquire write on rwlock1, then try to acquire write on rwlock2
        {
            let rwlock1 = rwlock1.clone();
            let rwlock2 = rwlock2.clone();
            async move {
                // Acquire write on first rwlock
                let mut write1 = rwlock1.write().await;
                *write1 += 1;

                // Try to acquire write on second rwlock - potential deadlock
                let mut write2 = rwlock2.write().await;
                *write2 += 1;

                drop(write2);
                drop(write1);
            }
            .primary()
            .spawn_named("Task 1");
        }

        // Task 2: acquire write on rwlock2, then try to acquire write on rwlock1
        {
            let rwlock1 = rwlock1.clone();
            let rwlock2 = rwlock2.clone();
            async move {
                // Acquire write on second rwlock
                let mut write2 = rwlock2.write().await;
                *write2 += 1;

                // Try to acquire write on first rwlock - potential deadlock
                let mut write1 = rwlock1.write().await;
                *write1 += 1;

                drop(write1);
                drop(write2);
            }
            .primary()
            .spawn_named("Task 2");
        }
    })();
}
