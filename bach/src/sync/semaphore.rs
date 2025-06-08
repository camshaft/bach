/// A wrapper around tokio's Semaphore that integrates with Bach's coop system.
///
/// This semaphore implementation ensures proper interleaving simulation with Bach's
/// cooperative scheduling system.
#[cfg(any(test, feature = "coop"))]
pub struct Semaphore {
    // Store the inner semaphore in an Arc to enable owned permit functionality
    // This is required because tokio's acquire_owned methods accept Arc<Semaphore> not just Semaphore
    inner: std::sync::Arc<tokio::sync::Semaphore>,
    acquire_op: crate::coop::Operation,
}

#[cfg(any(test, feature = "coop"))]
impl Semaphore {
    /// Creates a new semaphore with the given number of permits.
    pub fn new(permits: usize) -> Self {
        Self {
            inner: std::sync::Arc::new(tokio::sync::Semaphore::new(permits)),
            acquire_op: crate::coop::Operation::register(),
        }
    }

    /// Returns the current number of available permits.
    pub fn available_permits(&self) -> usize {
        self.inner.available_permits()
    }

    /// Adds `n` new permits to the semaphore.
    pub fn add_permits(&self, n: usize) {
        self.inner.add_permits(n);
    }

    /// Acquires a permit from the semaphore.
    ///
    /// Returns a SemaphorePermit that releases the permit when dropped.
    /// This method will register the acquire operation with Bach's coop system,
    /// ensuring proper interleaving exploration during simulation.
    pub async fn acquire(&self) -> Result<SemaphorePermit<'_>, tokio::sync::AcquireError> {
        use crate::tracing::Instrument;

        let span = crate::tracing::debug_span!("semaphore::acquire", count = 1);

        async {
            // First acquire the operation through the coop system
            self.acquire_op.acquire().await;

            // Then acquire the actual permit
            let permit = self.inner.acquire().await?;
            Ok(SemaphorePermit { permit })
        }
        .instrument(span)
        .await
    }

    /// Tries to acquire a permit from the semaphore without waiting.
    pub fn try_acquire(&self) -> Result<SemaphorePermit<'_>, tokio::sync::TryAcquireError> {
        let permit = self.inner.try_acquire()?;
        Ok(SemaphorePermit { permit })
    }

    /// Acquires `n` permits from the semaphore.
    pub async fn acquire_many(
        &self,
        n: u32,
    ) -> Result<SemaphorePermit<'_>, tokio::sync::AcquireError> {
        use crate::tracing::Instrument;

        let span = crate::tracing::debug_span!("semaphore::acquire", count = n);

        async {
            // First acquire the operation through the coop system
            self.acquire_op.acquire().await;

            let permit = self.inner.acquire_many(n).await?;

            Ok(SemaphorePermit { permit })
        }
        .instrument(span)
        .await
    }

    /// Tries to acquire `n` permits from the semaphore without waiting.
    pub fn try_acquire_many(
        &self,
        n: u32,
    ) -> Result<SemaphorePermit<'_>, tokio::sync::TryAcquireError> {
        let permit = self.inner.try_acquire_many(n)?;
        Ok(SemaphorePermit { permit })
    }

    /// Acquires a permit from the semaphore.
    ///
    /// Returns an OwnedSemaphorePermit that releases the permit when dropped.
    /// This method will register the acquire operation with Bach's coop system,
    /// ensuring proper interleaving exploration during simulation.
    pub async fn acquire_owned(
        self: std::sync::Arc<Self>,
    ) -> Result<OwnedSemaphorePermit, tokio::sync::AcquireError> {
        use crate::tracing::Instrument;

        let span = crate::tracing::debug_span!("semaphore::acquire_owned", count = 1);

        async {
            // First acquire the operation through the coop system
            self.acquire_op.acquire().await;

            // Use tokio's acquire_owned method with our already Arc-wrapped inner semaphore
            let permit = self.inner.clone().acquire_owned().await?;

            Ok(OwnedSemaphorePermit { permit })
        }
        .instrument(span)
        .await
    }

    /// Tries to acquire a permit from the semaphore without waiting.
    pub fn try_acquire_owned(
        self: std::sync::Arc<Self>,
    ) -> Result<OwnedSemaphorePermit, tokio::sync::TryAcquireError> {
        // Use tokio's try_acquire_owned method with our already Arc-wrapped inner semaphore
        let permit = self.inner.clone().try_acquire_owned()?;
        Ok(OwnedSemaphorePermit { permit })
    }

    /// Acquires `n` permits from the semaphore.
    pub async fn acquire_many_owned(
        self: std::sync::Arc<Self>,
        n: u32,
    ) -> Result<OwnedSemaphorePermit, tokio::sync::AcquireError> {
        use crate::tracing::Instrument;

        let span = crate::tracing::debug_span!("semaphore::acquire_many_owned", count = n);

        async {
            // First acquire the operation through the coop system
            self.acquire_op.acquire().await;

            // Use tokio's acquire_many_owned method with our already Arc-wrapped inner semaphore
            let permit = self.inner.clone().acquire_many_owned(n).await?;

            Ok(OwnedSemaphorePermit { permit })
        }
        .instrument(span)
        .await
    }

    /// Tries to acquire `n` permits from the semaphore without waiting.
    pub fn try_acquire_many_owned(
        self: std::sync::Arc<Self>,
        n: u32,
    ) -> Result<OwnedSemaphorePermit, tokio::sync::TryAcquireError> {
        // Use tokio's try_acquire_many_owned method with our already Arc-wrapped inner semaphore
        let permit = self.inner.clone().try_acquire_many_owned(n)?;
        Ok(OwnedSemaphorePermit { permit })
    }

    /// Closes the semaphore, causing all pending and future calls to acquire
    /// to return an error.
    pub fn close(&self) {
        // Close the semaphore
        self.inner.close();
    }
}

/// A permit from the semaphore.
///
/// This type is created by the [`acquire`](Semaphore::acquire) and
/// [`try_acquire`](Semaphore::try_acquire) methods.
#[cfg(any(test, feature = "coop"))]
pub struct SemaphorePermit<'a> {
    #[allow(dead_code)]
    permit: tokio::sync::SemaphorePermit<'a>,
}

/// An owned permit from the semaphore.
///
/// This type is created by the [`acquire_owned`](Semaphore::acquire_owned) and
/// [`try_acquire_owned`](Semaphore::try_acquire_owned) methods.
#[cfg(any(test, feature = "coop"))]
pub struct OwnedSemaphorePermit {
    #[allow(dead_code)]
    permit: tokio::sync::OwnedSemaphorePermit,
}

// Provide dummy implementations when coop is not enabled
#[cfg(not(any(test, feature = "coop")))]
pub struct Semaphore(std::marker::PhantomData<()>);

#[cfg(not(any(test, feature = "coop")))]
impl Semaphore {
    pub fn new(_permits: usize) -> Self {
        unimplemented!("Semaphore requires the coop feature")
    }
}

#[cfg(not(any(test, feature = "coop")))]
pub struct SemaphorePermit<'a>(std::marker::PhantomData<&'a ()>);

#[cfg(not(any(test, feature = "coop")))]
pub struct OwnedSemaphorePermit(std::marker::PhantomData<()>);
