/// A wrapper around tokio's RwLock that integrates with Bach's coop system.
///
/// This RwLock implementation ensures proper interleaving simulation with Bach's
/// cooperative scheduling system.
#[cfg(any(test, feature = "coop"))]
pub struct RwLock<T: ?Sized> {
    lock_op: crate::coop::Operation,
    inner: tokio::sync::RwLock<T>,
}

#[cfg(any(test, feature = "coop"))]
impl<T: ?Sized> RwLock<T> {
    /// Creates a new RwLock with the given value.
    pub fn new(value: T) -> Self
    where
        T: Sized,
    {
        Self {
            inner: tokio::sync::RwLock::new(value),
            lock_op: crate::coop::Operation::register(),
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
}

/// A read guard that releases the read lock when dropped.
#[cfg(any(test, feature = "coop"))]
pub struct RwLockReadGuard<'a, T: ?Sized> {
    guard: tokio::sync::RwLockReadGuard<'a, T>,
}

#[cfg(any(test, feature = "coop"))]
impl<'a, T: ?Sized> std::ops::Deref for RwLockReadGuard<'a, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &*self.guard
    }
}

/// A write guard that releases the write lock when dropped.
#[cfg(any(test, feature = "coop"))]
pub struct RwLockWriteGuard<'a, T: ?Sized> {
    guard: tokio::sync::RwLockWriteGuard<'a, T>,
}

#[cfg(any(test, feature = "coop"))]
impl<'a, T: ?Sized> std::ops::Deref for RwLockWriteGuard<'a, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &*self.guard
    }
}

#[cfg(any(test, feature = "coop"))]
impl<'a, T: ?Sized> std::ops::DerefMut for RwLockWriteGuard<'a, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut *self.guard
    }
}

// Provide a dummy implementation when coop is not enabled
#[cfg(not(any(test, feature = "coop")))]
pub struct RwLock<T>(std::marker::PhantomData<T>);

#[cfg(not(any(test, feature = "coop")))]
impl<T> RwLock<T> {
    pub fn new(_value: T) -> Self {
        unimplemented!("RwLock requires the coop feature")
    }
}

#[cfg(not(any(test, feature = "coop")))]
pub struct RwLockReadGuard<'a, T>(std::marker::PhantomData<&'a T>);

#[cfg(not(any(test, feature = "coop")))]
pub struct RwLockWriteGuard<'a, T>(std::marker::PhantomData<&'a mut T>);
