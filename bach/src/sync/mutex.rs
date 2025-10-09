#[cfg(any(test, feature = "coop"))]
mod coop_impl {
    use crate::coop::Operation;
    use std::sync::Arc;
    use tokio::sync::Mutex as TokioMutex;

    /// A wrapper around tokio's Mutex that integrates with Bach's coop system.
    ///
    /// This mutex implementation ensures proper interleaving simulation with Bach's
    /// cooperative scheduling system.
    pub struct Mutex<T: ?Sized> {
        lock_op: Operation,
        // Store the inner mutex in an Arc to enable owned_lock functionality
        // This is required because tokio's lock_owned methods accept Arc<Mutex> not just Mutex
        inner: Arc<TokioMutex<T>>,
    }

    impl<T: ?Sized> Mutex<T> {
        /// Creates a new mutex with the given value.
        pub fn new(value: T) -> Self
        where
            T: Sized,
        {
            Self {
                inner: Arc::new(TokioMutex::new(value)),
                lock_op: Operation::register(),
            }
        }

        /// Acquires the mutex.
        ///
        /// This method will register the lock operation with Bach's coop system,
        /// ensuring proper interleaving exploration during simulation.
        pub async fn lock(&self) -> MutexGuard<'_, T> {
            use crate::tracing::Instrument;

            let span = crate::tracing::debug_span!("mutex::lock");

            async {
                // First acquire the operation through the coop system
                self.lock_op.acquire().await;

                // Then acquire the actual lock
                let guard = self.inner.lock().await;

                MutexGuard { guard }
            }
            .instrument(span)
            .await
        }

        /// Attempts to acquire the lock without waiting.
        pub fn try_lock(&self) -> Result<MutexGuard<'_, T>, tokio::sync::TryLockError> {
            // Try to acquire the actual lock
            match self.inner.try_lock() {
                Ok(guard) => Ok(MutexGuard { guard }),
                Err(err) => Err(err),
            }
        }

        /// Acquires ownership of the mutex, returning an owned guard that can be held across await points.
        ///
        /// This method will register the lock operation with Bach's coop system,
        /// ensuring proper interleaving exploration during simulation.
        pub async fn lock_owned(self: Arc<Self>) -> OwnedMutexGuard<T>
        where
            T: Sized,
        {
            use crate::tracing::Instrument;

            let span = crate::tracing::debug_span!("mutex::lock_owned");

            async {
                // First acquire the operation through the coop system
                self.lock_op.acquire().await;

                // Use tokio's lock_owned method with our already Arc-wrapped inner mutex
                let guard = self.inner.clone().lock_owned().await;

                OwnedMutexGuard { guard }
            }
            .instrument(span)
            .await
        }

        /// Attempts to acquire the lock in an owned fashion without waiting.
        pub fn try_lock_owned(
            self: Arc<Self>,
        ) -> Result<OwnedMutexGuard<T>, tokio::sync::TryLockError>
        where
            T: Sized,
        {
            match self.inner.clone().try_lock_owned() {
                Ok(guard) => Ok(OwnedMutexGuard { guard }),
                Err(err) => Err(err),
            }
        }
    }

    /// A guard that releases the mutex when dropped.
    pub struct MutexGuard<'a, T: ?Sized> {
        guard: tokio::sync::MutexGuard<'a, T>,
    }

    impl<'a, T: ?Sized> std::ops::Deref for MutexGuard<'a, T> {
        type Target = T;

        fn deref(&self) -> &Self::Target {
            &self.guard
        }
    }

    impl<'a, T: ?Sized> std::ops::DerefMut for MutexGuard<'a, T> {
        fn deref_mut(&mut self) -> &mut Self::Target {
            &mut self.guard
        }
    }

    /// An owned guard that releases the mutex when dropped.
    pub struct OwnedMutexGuard<T: ?Sized> {
        guard: tokio::sync::OwnedMutexGuard<T>,
    }

    impl<T: ?Sized> std::ops::Deref for OwnedMutexGuard<T> {
        type Target = T;

        fn deref(&self) -> &Self::Target {
            &self.guard
        }
    }

    impl<T: ?Sized> std::ops::DerefMut for OwnedMutexGuard<T> {
        fn deref_mut(&mut self) -> &mut Self::Target {
            &mut self.guard
        }
    }
}

// When the coop feature is enabled, export our wrapped implementation
#[cfg(any(test, feature = "coop"))]
pub use coop_impl::*;

// Otherwise, re-export tokio's mutex types directly
#[cfg(not(any(test, feature = "coop")))]
pub use tokio::sync::{Mutex, MutexGuard};
