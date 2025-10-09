#[cfg(any(test, feature = "coop"))]
mod coop_impl {
    use crate::coop::Operation;
    use tokio::sync::mpsc;

    pub use mpsc::error;

    pub struct Sender<T> {
        send_op: Operation,
        inner: mpsc::Sender<T>,
    }

    pub struct Receiver<T> {
        recv_op: Operation,
        inner: mpsc::Receiver<T>,
    }

    pub struct UnboundedSender<T> {
        send_op: Operation,
        inner: mpsc::UnboundedSender<T>,
    }

    pub struct UnboundedReceiver<T> {
        recv_op: Operation,
        inner: mpsc::UnboundedReceiver<T>,
    }

    pub struct WeakSender<T> {
        send_op: Operation,
        inner: mpsc::WeakSender<T>,
    }

    pub struct WeakUnboundedSender<T> {
        send_op: Operation,
        inner: mpsc::WeakUnboundedSender<T>,
    }

    pub struct Permit<'a, T> {
        inner: mpsc::Permit<'a, T>,
    }

    pub struct OwnedPermit<T> {
        inner: mpsc::OwnedPermit<T>,
        send_op: Operation,
    }

    pub struct PermitIterator<'a, T> {
        inner: mpsc::PermitIterator<'a, T>,
    }

    impl<T> Sender<T> {
        /// Sends a value, waiting until there is capacity.
        pub async fn send(&self, value: T) -> Result<(), error::SendError<T>> {
            use crate::tracing::Instrument;

            let span = crate::tracing::debug_span!("mpsc::send");

            async {
                // First acquire the operation through the coop system
                self.send_op.acquire().await;

                // Then perform the actual send
                self.inner.send(value).await
            }
            .instrument(span)
            .await
        }

        /// Tries to send a value immediately.
        pub fn try_send(&self, value: T) -> Result<(), error::TrySendError<T>> {
            self.inner.try_send(value)
        }

        /// Reserves capacity to send a value.
        pub async fn reserve(&self) -> Result<Permit<'_, T>, error::SendError<()>> {
            use crate::tracing::Instrument;

            let span = crate::tracing::debug_span!("mpsc::reserve");

            async {
                // First acquire the operation through the coop system
                self.send_op.acquire().await;

                // Then perform the actual reserve
                self.inner.reserve().await.map(|inner| Permit { inner })
            }
            .instrument(span)
            .await
        }

        /// Tries to reserve capacity to send a value without waiting.
        pub fn try_reserve(&self) -> Result<Permit<'_, T>, error::TrySendError<()>> {
            self.inner.try_reserve().map(|inner| Permit { inner })
        }

        /// Reserves capacity to send n values.
        pub async fn reserve_many(
            &self,
            n: usize,
        ) -> Result<PermitIterator<'_, T>, error::SendError<()>> {
            use crate::tracing::Instrument;

            let span = crate::tracing::debug_span!("mpsc::reserve_many");

            async {
                // First acquire the operation through the coop system
                // For reserve_many we'll just acquire once, not n times
                self.send_op.acquire().await;

                // Then perform the actual reserve_many
                self.inner
                    .reserve_many(n)
                    .await
                    .map(|inner| PermitIterator { inner })
            }
            .instrument(span)
            .await
        }

        /// Returns `true` if the channel is closed.
        pub fn is_closed(&self) -> bool {
            self.inner.is_closed()
        }

        /// Returns the current capacity of the channel.
        pub fn capacity(&self) -> usize {
            self.inner.capacity()
        }

        /// Returns true if the send half of the channel is closed.
        pub fn same_channel(&self, other: &Self) -> bool {
            self.inner.same_channel(&other.inner)
        }

        /// Creates a new `WeakSender` for this channel.
        pub fn downgrade(&self) -> WeakSender<T> {
            WeakSender {
                inner: self.inner.downgrade(),
                send_op: self.send_op,
            }
        }
    }

    impl<T> Clone for Sender<T> {
        fn clone(&self) -> Self {
            Self {
                inner: self.inner.clone(),
                send_op: self.send_op,
            }
        }
    }

    impl<T> Receiver<T> {
        /// Receives the next value for this receiver.
        pub async fn recv(&mut self) -> Option<T> {
            use crate::tracing::Instrument;

            let span = crate::tracing::debug_span!("mpsc::recv");

            async {
                // First acquire the operation through the coop system
                self.recv_op.acquire().await;

                // Then perform the actual receive
                self.inner.recv().await
            }
            .instrument(span)
            .await
        }

        /// Attempts to receive a value from the channel without blocking.
        pub fn try_recv(&mut self) -> Result<T, error::TryRecvError> {
            self.inner.try_recv()
        }

        /// Closes the receiving half of a channel.
        pub fn close(&mut self) {
            self.inner.close()
        }
    }

    impl<T> UnboundedSender<T> {
        /// Sends a value through the channel.
        pub fn send(&self, value: T) -> Result<(), error::SendError<T>> {
            // The unbounded sender doesn't block right now
            // TODO figure out how to integrate this with the coop system while
            //      still preserving a senders message ordering
            self.inner.send(value)
        }

        /// Returns `true` if the channel is closed.
        pub fn is_closed(&self) -> bool {
            self.inner.is_closed()
        }

        /// Returns true if the send half of the channel is closed.
        pub fn same_channel(&self, other: &Self) -> bool {
            self.inner.same_channel(&other.inner)
        }

        /// Creates a new `WeakUnboundedSender` for this channel.
        pub fn downgrade(&self) -> WeakUnboundedSender<T> {
            WeakUnboundedSender {
                inner: self.inner.downgrade(),
                send_op: self.send_op,
            }
        }
    }

    impl<T> Clone for UnboundedSender<T> {
        fn clone(&self) -> Self {
            Self {
                inner: self.inner.clone(),
                send_op: self.send_op,
            }
        }
    }

    impl<T> UnboundedReceiver<T> {
        /// Receives the next value for this receiver.
        pub async fn recv(&mut self) -> Option<T> {
            use crate::tracing::Instrument;

            let span = crate::tracing::debug_span!("mpsc::unbounded_recv");

            async {
                // First acquire the operation through the coop system
                self.recv_op.acquire().await;

                // Then perform the actual receive
                self.inner.recv().await
            }
            .instrument(span)
            .await
        }

        /// Attempts to receive a value from the channel without blocking.
        pub fn try_recv(&mut self) -> Result<T, error::TryRecvError> {
            self.inner.try_recv()
        }

        /// Closes the receiving half of a channel.
        pub fn close(&mut self) {
            self.inner.close()
        }
    }

    impl<'a, T> Permit<'a, T> {
        /// Sends a value using the permit.
        pub fn send(self, value: T) {
            self.inner.send(value)
        }
    }

    impl<T> OwnedPermit<T> {
        /// Sends a value using the permit.
        pub fn send(self, value: T) -> Sender<T> {
            let inner = self.inner.send(value);
            let send_op = self.send_op;
            Sender { inner, send_op }
        }
    }

    impl<'a, T> Iterator for PermitIterator<'a, T> {
        type Item = Permit<'a, T>;

        fn next(&mut self) -> Option<Self::Item> {
            self.inner.next().map(|inner| Permit { inner })
        }
    }

    impl<T> WeakSender<T> {
        /// Attempts to upgrade the `WeakSender` to a `Sender`.
        pub fn upgrade(&self) -> Option<Sender<T>> {
            self.inner.upgrade().map(|inner| Sender {
                inner,
                send_op: self.send_op,
            })
        }
    }

    impl<T> Clone for WeakSender<T> {
        fn clone(&self) -> Self {
            Self {
                inner: self.inner.clone(),
                send_op: self.send_op,
            }
        }
    }

    impl<T> WeakUnboundedSender<T> {
        /// Attempts to upgrade the `WeakUnboundedSender` to an `UnboundedSender`.
        pub fn upgrade(&self) -> Option<UnboundedSender<T>> {
            self.inner.upgrade().map(|inner| UnboundedSender {
                inner,
                send_op: self.send_op,
            })
        }
    }

    impl<T> Clone for WeakUnboundedSender<T> {
        fn clone(&self) -> Self {
            Self {
                inner: self.inner.clone(),
                send_op: self.send_op,
            }
        }
    }

    /// Creates a bounded mpsc channel for communicating between asynchronous tasks with backpressure.
    pub fn channel<T>(buffer: usize) -> (Sender<T>, Receiver<T>) {
        let (tx, rx) = mpsc::channel(buffer);

        let tx = Sender {
            inner: tx,
            send_op: Operation::register(),
        };

        let rx = Receiver {
            inner: rx,
            recv_op: Operation::register(),
        };

        (tx, rx)
    }

    /// Creates an unbounded mpsc channel for communicating between asynchronous tasks without backpressure.
    pub fn unbounded_channel<T>() -> (UnboundedSender<T>, UnboundedReceiver<T>) {
        let (tx, rx) = mpsc::unbounded_channel();

        let tx = UnboundedSender {
            inner: tx,
            send_op: Operation::register(),
        };

        let rx = UnboundedReceiver {
            inner: rx,
            recv_op: Operation::register(),
        };

        (tx, rx)
    }
}

// When the coop feature is enabled, export our wrapped implementation
#[cfg(any(test, feature = "coop"))]
pub use coop_impl::*;

// Otherwise, re-export tokio's mpsc module directly
#[cfg(not(any(test, feature = "coop")))]
pub use tokio::sync::mpsc::*;
