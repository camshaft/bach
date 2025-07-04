#[cfg(any(test, feature = "coop"))]
mod coop_impl {
    use crate::coop::Operation;
    use std::sync::Arc;
    use tokio::sync::RwLock as TokioRwLock;

    /// A wrapper around tokio's RwLock that integrates with Bach's coop system.
    ///
    /// This RwLock implementation ensures proper interleaving simulation with Bach's
    /// cooperative scheduling system.
    pub struct RwLock<T: ?Sized> {
        lock_op: Operation,
        // Store the inner rwlock in an Arc to enable owned_* methods
        // This is required because tokio's read/write_owned methods accept Arc<RwLock> not just RwLock
        inner: Arc<TokioRwLock<T>>,
    }

    impl<T: ?Sized> RwLock<T> {
        /// Creates a new RwLock with the given value.
        pub fn new(value: T) -> Self
        where
            T: Sized,
        {
            Self {
                inner: Arc::new(TokioRwLock::new(value)),
                lock_op: Operation::register(),
            }
        }

        /// Acquires a read lock on the RwLock.
        ///
        /// This method will register the read lock operation with Bach's coop system,
        /// ensuring proper interleaving exploration during simulation.
        pub async fn read(&self) -> RwLockReadGuard<'_, T> {
            use crate::tracing::Instrument;

            let span = crate::tracing::debug_span!("rwlock::read");

            async {
                // First acquire the operation through the coop system
                self.lock_op.acquire().await;

                // Then acquire the actual read lock
                let guard = self.inner.read().await;

                RwLockReadGuard { guard }
            }
            .instrument(span)
            .await
        }

        /// Tries to acquire a read lock on the RwLock without waiting.
        pub fn try_read(&self) -> Result<RwLockReadGuard<'_, T>, tokio::sync::TryLockError> {
            // Try to acquire the actual read lock
            match self.inner.try_read() {
                Ok(guard) => Ok(RwLockReadGuard { guard }),
                Err(err) => Err(err),
            }
        }

        /// Acquires a write lock on the RwLock.
        ///
        /// This method will register the write lock operation with Bach's coop system,
        /// ensuring proper interleaving exploration during simulation.
        pub async fn write(&self) -> RwLockWriteGuard<'_, T> {
            use crate::tracing::Instrument;

            let span = crate::tracing::debug_span!("rwlock::write");

            async {
                // First acquire the operation through the coop system
                self.lock_op.acquire().await;

                // Then acquire the actual write lock
                let guard = self.inner.write().await;

                RwLockWriteGuard { guard }
            }
            .instrument(span)
            .await
        }

        /// Tries to acquire a write lock on the RwLock without waiting.
        pub fn try_write(&self) -> Result<RwLockWriteGuard<'_, T>, tokio::sync::TryLockError> {
            // Try to acquire the actual write lock
            match self.inner.try_write() {
                Ok(guard) => Ok(RwLockWriteGuard { guard }),
                Err(err) => Err(err),
            }
        }

        /// Acquires a read lock on an Arc-wrapped RwLock, returning an owned guard that can be
        /// held across await points.
        ///
        /// This method will register the read lock operation with Bach's coop system,
        /// ensuring proper interleaving exploration during simulation.
        pub async fn read_owned(self: Arc<Self>) -> OwnedRwLockReadGuard<T>
        where
            T: Sized,
        {
            use crate::tracing::Instrument;

            let span = crate::tracing::debug_span!("rwlock::read_owned");

            async {
                // First acquire the operation through the coop system
                self.lock_op.acquire().await;

                // Then use tokio's read_owned method with our already Arc-wrapped inner rwlock
                let guard = self.inner.clone().read_owned().await;

                OwnedRwLockReadGuard { guard }
            }
            .instrument(span)
            .await
        }

        /// Tries to acquire an owned read lock on the RwLock without waiting.
        pub fn try_read_owned(
            self: Arc<Self>,
        ) -> Result<OwnedRwLockReadGuard<T>, tokio::sync::TryLockError>
        where
            T: Sized,
        {
            match self.inner.clone().try_read_owned() {
                Ok(guard) => Ok(OwnedRwLockReadGuard { guard }),
                Err(err) => Err(err),
            }
        }

        /// Acquires a write lock on an Arc-wrapped RwLock, returning an owned guard that can be
        /// held across await points.
        ///
        /// This method will register the write lock operation with Bach's coop system,
        /// ensuring proper interleaving exploration during simulation.
        pub async fn write_owned(self: Arc<Self>) -> OwnedRwLockWriteGuard<T>
        where
            T: Sized,
        {
            use crate::tracing::Instrument;

            let span = crate::tracing::debug_span!("rwlock::write_owned");

            async {
                // First acquire the operation through the coop system
                self.lock_op.acquire().await;

                // Then use tokio's write_owned method with our already Arc-wrapped inner rwlock
                let guard = self.inner.clone().write_owned().await;

                OwnedRwLockWriteGuard { guard }
            }
            .instrument(span)
            .await
        }

        /// Tries to acquire an owned write lock on the RwLock without waiting.
        pub fn try_write_owned(
            self: Arc<Self>,
        ) -> Result<OwnedRwLockWriteGuard<T>, tokio::sync::TryLockError>
        where
            T: Sized,
        {
            match self.inner.clone().try_write_owned() {
                Ok(guard) => Ok(OwnedRwLockWriteGuard { guard }),
                Err(err) => Err(err),
            }
        }
    }

    /// A read guard that releases the read lock when dropped.
    pub struct RwLockReadGuard<'a, T: ?Sized> {
        guard: tokio::sync::RwLockReadGuard<'a, T>,
    }

    impl<'a, T: ?Sized> std::ops::Deref for RwLockReadGuard<'a, T> {
        type Target = T;

        fn deref(&self) -> &Self::Target {
            &*self.guard
        }
    }

    /// A write guard that releases the write lock when dropped.
    pub struct RwLockWriteGuard<'a, T: ?Sized> {
        guard: tokio::sync::RwLockWriteGuard<'a, T>,
    }

    impl<'a, T: ?Sized> std::ops::Deref for RwLockWriteGuard<'a, T> {
        type Target = T;

        fn deref(&self) -> &Self::Target {
            &*self.guard
        }
    }

    impl<'a, T: ?Sized> std::ops::DerefMut for RwLockWriteGuard<'a, T> {
        fn deref_mut(&mut self) -> &mut Self::Target {
            &mut *self.guard
        }
    }

    /// An owned read guard that releases the read lock when dropped.
    pub struct OwnedRwLockReadGuard<T: ?Sized> {
        guard: tokio::sync::OwnedRwLockReadGuard<T>,
    }

    impl<T: ?Sized> std::ops::Deref for OwnedRwLockReadGuard<T> {
        type Target = T;

        fn deref(&self) -> &Self::Target {
            &*self.guard
        }
    }

    /// An owned write guard that releases the write lock when dropped.
    pub struct OwnedRwLockWriteGuard<T: ?Sized> {
        guard: tokio::sync::OwnedRwLockWriteGuard<T>,
    }

    impl<T: ?Sized> std::ops::Deref for OwnedRwLockWriteGuard<T> {
        type Target = T;

        fn deref(&self) -> &Self::Target {
            &*self.guard
        }
    }

    impl<T: ?Sized> std::ops::DerefMut for OwnedRwLockWriteGuard<T> {
        fn deref_mut(&mut self) -> &mut Self::Target {
            &mut *self.guard
        }
    }
}

// When the coop feature is enabled, export our wrapped implementation
#[cfg(any(test, feature = "coop"))]
pub use coop_impl::*;

// Otherwise, re-export tokio's rwlock types directly
#[cfg(not(any(test, feature = "coop")))]
pub use tokio::sync::{RwLock, RwLockReadGuard, RwLockWriteGuard};
