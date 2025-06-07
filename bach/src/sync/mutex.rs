/// A wrapper around tokio's Mutex that integrates with Bach's coop system.
///
/// This mutex implementation ensures proper interleaving simulation with Bach's
/// cooperative scheduling system.
#[cfg(any(test, feature = "coop"))]
pub struct Mutex<T: ?Sized> {
    lock_op: crate::coop::Operation,
    inner: tokio::sync::Mutex<T>,
}

#[cfg(any(test, feature = "coop"))]
impl<T: ?Sized> Mutex<T> {
    /// Creates a new mutex with the given value.
    pub fn new(value: T) -> Self
    where
        T: Sized,
    {
        Self {
            inner: tokio::sync::Mutex::new(value),
            lock_op: crate::coop::Operation::register(),
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
}

/// A guard that releases the mutex when dropped.
#[cfg(any(test, feature = "coop"))]
pub struct MutexGuard<'a, T: ?Sized> {
    guard: tokio::sync::MutexGuard<'a, T>,
}

#[cfg(any(test, feature = "coop"))]
impl<'a, T: ?Sized> std::ops::Deref for MutexGuard<'a, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &*self.guard
    }
}

#[cfg(any(test, feature = "coop"))]
impl<'a, T: ?Sized> std::ops::DerefMut for MutexGuard<'a, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut *self.guard
    }
}

// Provide a dummy implementation when coop is not enabled
#[cfg(not(any(test, feature = "coop")))]
pub struct Mutex<T>(std::marker::PhantomData<T>);

#[cfg(not(any(test, feature = "coop")))]
impl<T> Mutex<T> {
    pub fn new(_value: T) -> Self {
        unimplemented!("Mutex requires the coop feature")
    }
}

#[cfg(not(any(test, feature = "coop")))]
pub struct MutexGuard<'a, T>(std::marker::PhantomData<&'a T>);
