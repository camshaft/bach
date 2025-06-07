use crate::testing::Log;

use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Event {
    Start,
    SemaphoreAcquired { task: usize, permits: u32 },
    SemaphoreReleased { task: usize, permits: u32 },
}

impl crate::testing::Event for Event {
    fn is_start(&self) -> bool {
        matches!(self, Event::Start)
    }
}

#[test]
fn semaphore_permits() {
    bolero::check!().exhaustive().run(sim(|| {
        async {
            let semaphore = Semaphore::new(5);

            // Check initial permits
            assert_eq!(semaphore.available_permits(), 5);

            // Acquire some permits
            let permit1 = semaphore.acquire().await.unwrap();
            assert_eq!(semaphore.available_permits(), 4);

            // Add more permits
            semaphore.add_permits(3);
            assert_eq!(semaphore.available_permits(), 7);

            // Release the permit and check again
            drop(permit1);
            assert_eq!(semaphore.available_permits(), 8);
        }
        .primary()
        .spawn();
    }));
}

#[test]
fn semaphore_try_acquire() {
    bolero::check!().exhaustive().run(sim(|| {
        async {
            let semaphore = Semaphore::new(2);

            // Successfully try_acquire
            let permit1 = semaphore.try_acquire().unwrap();
            let permit2 = semaphore.try_acquire().unwrap();

            // This should fail
            assert!(semaphore.try_acquire().is_err());

            // Release one permit
            drop(permit1);

            // Now try_acquire should succeed again
            let _permit3 = semaphore.try_acquire().unwrap();

            // Clean up
            drop(permit2);
        }
        .primary()
        .spawn();
    }));
}

#[test]
fn semaphore_acquire_many() {
    bolero::check!().exhaustive().run(sim(|| {
        async {
            let semaphore = Arc::new(Semaphore::new(5));

            // Acquire multiple permits at once
            let permit = semaphore.acquire_many(3).await.unwrap();
            assert_eq!(semaphore.available_permits(), 2);

            // In a separate task to avoid deadlock
            {
                let semaphore = semaphore.clone();
                async move {
                    // Try to acquire too many
                    let too_many = semaphore.acquire_many(3).await.unwrap();

                    // Wait a bit
                    bach::time::sleep(std::time::Duration::from_millis(10)).await;

                    // Release the original permit
                    drop(too_many);
                }
                .primary()
                .spawn();
            }

            10.ms().sleep().await;
            drop(permit);
        }
        .primary()
        .spawn();
    }));
}

#[test]
fn semaphore_try_acquire_many() {
    bolero::check!().exhaustive().run(sim(|| {
        async {
            let semaphore = Semaphore::new(5);

            // Successfully try to acquire multiple permits
            let permit = semaphore.try_acquire_many(3).unwrap();
            assert_eq!(semaphore.available_permits(), 2);

            // This should fail
            assert!(semaphore.try_acquire_many(3).is_err());

            // This should succeed
            let _permit2 = semaphore.try_acquire_many(2).unwrap();
            assert_eq!(semaphore.available_permits(), 0);

            // Release first permit
            drop(permit);
            assert_eq!(semaphore.available_permits(), 3);
        }
        .primary()
        .spawn();
    }));
}

#[test]
fn semaphore_close() {
    bolero::check!().exhaustive().run(sim(|| {
        async {
            let semaphore = Semaphore::new(5);

            // Close the semaphore
            semaphore.close();

            // Further acquisitions should fail
            assert!(semaphore.try_acquire().is_err());

            // Async acquisition should also fail
            match semaphore.acquire().await {
                Err(_) => {} // Expected
                Ok(_) => panic!("Expected acquisition to fail after close"),
            };
        }
        .primary()
        .spawn();
    }));
}

#[test]
fn semaphore_interleavings_snapshot() {
    static LOG: Log<Event> = Log::new();

    bolero::check!().exhaustive().run(sim(|| {
        // Start a new event sequence
        LOG.push(Event::Start);

        // Create a semaphore with 2 permits
        let semaphore = Arc::new(Semaphore::new(2));

        // Launch multiple tasks that will contend for the semaphore
        for task_id in 0..4 {
            let semaphore = semaphore.clone();
            let permits = if task_id % 2 == 0 { 1u32 } else { 2u32 };

            async move {
                // Acquire permits
                if permits == 1 {
                    let _ = semaphore.acquire().await.unwrap();

                    LOG.push(Event::SemaphoreAcquired {
                        task: task_id,
                        permits,
                    });

                    // Do some work with the permit
                    bach::time::sleep(std::time::Duration::from_millis(5)).await;

                    // Release the permit (implicit when dropped)
                    LOG.push(Event::SemaphoreReleased {
                        task: task_id,
                        permits,
                    });
                } else {
                    let _ = semaphore.acquire_many(2).await.unwrap();

                    LOG.push(Event::SemaphoreAcquired {
                        task: task_id,
                        permits,
                    });

                    // Do some work with the permit
                    bach::time::sleep(std::time::Duration::from_millis(5)).await;

                    // Release the permit (implicit when dropped)
                    LOG.push(Event::SemaphoreReleased {
                        task: task_id,
                        permits,
                    });
                }
            }
            .primary()
            .spawn_named(format!("Task {task_id}"));
        }
    }));

    insta::assert_debug_snapshot!(LOG.check());
}

#[test]
#[should_panic = "Runtime stalled"]
fn semaphore_deadlock_detection() {
    sim(|| {
        // Create semaphores - one with limited permits
        let semaphore1 = Arc::new(Semaphore::new(1)); // Only 1 permit available
        let semaphore2 = Arc::new(Semaphore::new(1)); // Only 1 permit available

        // Task 1: acquire semaphore1, then try to acquire semaphore2
        {
            let semaphore1 = semaphore1.clone();
            let semaphore2 = semaphore2.clone();
            async move {
                // Acquire the first semaphore
                let permit1 = semaphore1.acquire().await.unwrap();

                // Try to acquire the second semaphore - this can deadlock
                // if task 2 is already holding semaphore2
                let permit2 = semaphore2.acquire().await.unwrap();

                // Release both semaphores
                drop(permit2);
                drop(permit1);
            }
            .primary()
            .spawn_named("Task 1");
        }

        // Task 2: acquire semaphore2, then try to acquire semaphore1
        {
            let semaphore1 = semaphore1.clone();
            let semaphore2 = semaphore2.clone();
            async move {
                // Acquire the second semaphore
                let permit2 = semaphore2.acquire().await.unwrap();
                // Try to acquire the first semaphore - this can deadlock
                // if task 1 is already holding semaphore1
                let permit1 = semaphore1.acquire().await.unwrap();
                // Release both semaphores
                drop(permit1);
                drop(permit2);
            }
            .primary()
            .spawn_named("Task 2");
        }
    })();
}
