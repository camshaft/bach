use crate::{
    coop::Operation,
    sync::queue::{CloseError, PopError, PushError, Queue},
};
use alloc::sync::Arc;
use core::{
    fmt,
    future::Future,
    marker::{PhantomData, PhantomPinned},
    pin::Pin,
    sync::atomic::{AtomicUsize, Ordering},
    task::{Context, Poll},
};
use event_listener_strategy::{
    easy_wrapper,
    event_listener::{Event, EventListener},
    EventListenerFuture, Strategy,
};
use futures_core::{ready, stream::Stream};
use pin_project_lite::pin_project;
use std::process::abort;

struct Channel<T, Q: ?Sized = dyn 'static + Send + Sync + Queue<T>> {
    /// Send operations waiting while the channel is full.
    send_ops: Event,

    /// Receive operations waiting while the channel is empty and not closed.
    recv_ops: Event,

    /// Stream operations while the channel is empty and not closed.
    stream_ops: Event,

    /// The number of currently active `Sender`s.
    sender_count: AtomicUsize,

    /// The number of currently active `Receivers`s.
    receiver_count: AtomicUsize,

    value: PhantomData<T>,

    send_resource: Operation,
    recv_resource: Operation,

    queue: Q,
}

impl<T> Channel<T> {
    /// Closes the channel and notifies all blocked operations.
    fn close(&self) -> Result<(), CloseError> {
        self.queue.close()?;
        // Notify all send operations.
        self.send_ops.notify(usize::MAX);

        // Notify all receive and stream operations.
        self.recv_ops.notify(usize::MAX);
        self.stream_ops.notify(usize::MAX);

        Ok(())
    }
}

/// Creates a channel.
pub fn new<Q, T>(queue: Q) -> (Sender<T>, Receiver<T>)
where
    Q: 'static + Send + Sync + Queue<T>,
{
    let channel = Arc::new(Channel {
        send_ops: Event::new(),
        recv_ops: Event::new(),
        stream_ops: Event::new(),
        sender_count: AtomicUsize::new(1),
        receiver_count: AtomicUsize::new(1),
        value: PhantomData,
        send_resource: Operation::register(),
        recv_resource: Operation::register(),
        queue,
    });

    let s = Sender {
        channel: channel.clone(),
    };
    let r = Receiver {
        listener: None,
        channel,
        _pin: PhantomPinned,
    };
    (s, r)
}

/// The sending side of a channel.
///
/// Senders can be cloned and shared among tasks. When all senders associated with a channel are
/// dropped, the channel becomes closed.
///
/// The channel can also be closed manually by calling [`Sender::close()`].
pub struct Sender<T> {
    /// Inner channel state.
    channel: Arc<Channel<T>>,
}

impl<T> Sender<T> {
    /// Attempts to push a message into the channel.
    pub fn try_push(&self, msg: T) -> Result<Option<T>, PushError<T>> {
        let prev = self.channel.queue.push(msg)?;

        // Notify a blocked receive operation. If the notified operation gets canceled,
        // it will notify another blocked receive operation.
        self.channel.recv_ops.notify_additional(1);

        // Notify all blocked streams.
        self.channel.stream_ops.notify(usize::MAX);

        Ok(prev)
    }

    /// Pushes a message into the channel.
    pub async fn push(&self, msg: T) -> Result<(), PushError<T>> {
        self.channel.send_resource.acquire().await;

        Push::_new(PushInner {
            sender: self,
            msg: Some(msg),
            listener: None,
            _pin: PhantomPinned,
        })
        .await
    }

    pub async fn send(&self, msg: T) -> Result<(), PushError<T>> {
        self.push(msg).await
    }

    /// Closes the channel.
    pub fn close(&self) -> Result<(), CloseError> {
        self.channel.close()
    }

    /// Returns `true` if the channel is closed.
    pub fn is_closed(&self) -> bool {
        self.channel.queue.is_closed()
    }

    /// Returns `true` if the channel is empty.
    pub fn is_empty(&self) -> bool {
        self.channel.queue.is_empty()
    }

    /// Returns `true` if the channel is full.
    pub fn is_full(&self) -> bool {
        self.channel.queue.is_full()
    }

    /// Returns the number of messages in the channel.
    pub fn len(&self) -> usize {
        self.channel.queue.len()
    }

    /// Returns the channel capacity if it's bounded.
    pub fn capacity(&self) -> Option<usize> {
        self.channel.queue.capacity()
    }

    /// Returns the number of receivers for the channel.
    pub fn receiver_count(&self) -> usize {
        self.channel.receiver_count.load(Ordering::SeqCst)
    }

    /// Returns the number of senders for the channel.
    pub fn sender_count(&self) -> usize {
        self.channel.sender_count.load(Ordering::SeqCst)
    }

    /// Downgrade the sender to a weak reference.
    pub fn downgrade(&self) -> WeakSender<T> {
        WeakSender {
            channel: self.channel.clone(),
        }
    }

    /// Returns whether the senders belong to the same channel.
    pub fn same_channel(&self, other: &Sender<T>) -> bool {
        Arc::ptr_eq(&self.channel, &other.channel)
    }
}

impl<T> Drop for Sender<T> {
    fn drop(&mut self) {
        // Decrement the sender count and close the channel if it drops down to zero.
        if self.channel.sender_count.fetch_sub(1, Ordering::AcqRel) == 1 {
            let _ = self.channel.close();
        }
    }
}

impl<T> fmt::Debug for Sender<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Sender {{ .. }}")
    }
}

impl<T> Clone for Sender<T> {
    fn clone(&self) -> Sender<T> {
        let count = self.channel.sender_count.fetch_add(1, Ordering::Relaxed);

        // Make sure the count never overflows, even if lots of sender clones are leaked.
        if count > usize::MAX / 2 {
            abort();
        }

        Sender {
            channel: self.channel.clone(),
        }
    }
}

pin_project! {
    /// The receiving side of a channel.
    ///
    /// Receivers can be cloned and shared among threads. When all receivers associated with a channel
    /// are dropped, the channel becomes closed.
    ///
    /// The channel can also be closed manually by calling [`Receiver::close()`].
    ///
    /// Receivers implement the [`Stream`] trait.
    pub struct Receiver<T> {
        // Inner channel state.
        channel: Arc<Channel<T>>,

        // Listens for a send or close event to unblock this stream.
        listener: Option<EventListener>,

        // Keeping this type `!Unpin` enables future optimizations.
        #[pin]
        _pin: PhantomPinned
    }

    impl<T> PinnedDrop for Receiver<T> {
        fn drop(this: Pin<&mut Self>) {
            let this = this.project();

            // Decrement the receiver count and close the channel if it drops down to zero.
            if this.channel.receiver_count.fetch_sub(1, Ordering::AcqRel) == 1 {
                let _ = this.channel.close();
            }
        }
    }
}

impl<T> Receiver<T> {
    /// Attempts to pop a message from the channel.
    pub fn try_pop(&self) -> Result<T, PopError> {
        let msg = self.channel.queue.pop()?;
        // Notify a blocked send operation. If the notified operation gets canceled, it
        // will notify another blocked send operation.
        self.channel.send_ops.notify_additional(1);
        Ok(msg)
    }

    /// Pops a message from the channel.
    pub async fn pop(&self) -> Result<T, PopError> {
        self.channel.recv_resource.acquire().await;

        Pop::_new(PopInner {
            receiver: self,
            listener: None,
            _pin: PhantomPinned,
        })
        .await
    }

    pub async fn recv(&self) -> Result<T, PopError> {
        self.pop().await
    }

    /// Closes the channel.
    pub fn close(&self) -> Result<(), CloseError> {
        self.channel.close()
    }

    /// Returns `true` if the channel is closed.
    pub fn is_closed(&self) -> bool {
        self.channel.queue.is_closed()
    }

    /// Returns `true` if the channel is empty.
    pub fn is_empty(&self) -> bool {
        self.channel.queue.is_empty()
    }

    /// Returns `true` if the channel is full.
    pub fn is_full(&self) -> bool {
        self.channel.queue.is_full()
    }

    /// Returns the number of messages in the channel.
    pub fn len(&self) -> usize {
        self.channel.queue.len()
    }

    /// Returns the channel capacity if it's bounded.
    pub fn capacity(&self) -> Option<usize> {
        self.channel.queue.capacity()
    }

    /// Returns the number of receivers for the channel.
    pub fn receiver_count(&self) -> usize {
        self.channel.receiver_count.load(Ordering::SeqCst)
    }

    /// Returns the number of senders for the channel.
    pub fn sender_count(&self) -> usize {
        self.channel.sender_count.load(Ordering::SeqCst)
    }

    /// Downgrade the receiver to a weak reference.
    pub fn downgrade(&self) -> WeakReceiver<T> {
        WeakReceiver {
            channel: self.channel.clone(),
        }
    }

    /// Returns whether the receivers belong to the same channel.
    pub fn same_channel(&self, other: &Receiver<T>) -> bool {
        Arc::ptr_eq(&self.channel, &other.channel)
    }
}

impl<T> fmt::Debug for Receiver<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Receiver {{ .. }}")
    }
}

impl<T> Clone for Receiver<T> {
    fn clone(&self) -> Receiver<T> {
        let count = self.channel.receiver_count.fetch_add(1, Ordering::Relaxed);

        // Make sure the count never overflows, even if lots of receiver clones are leaked.
        if count > usize::MAX / 2 {
            abort();
        }

        Receiver {
            channel: self.channel.clone(),
            listener: None,
            _pin: PhantomPinned,
        }
    }
}

impl<T> Stream for Receiver<T> {
    type Item = T;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        loop {
            // If this stream is listening for events, first wait for a notification.
            {
                let this = self.as_mut().project();
                if let Some(listener) = this.listener.as_mut() {
                    ready!(Pin::new(listener).poll(cx));
                    *this.listener = None;
                }
            }

            loop {
                // Attempt to receive a message.
                match self.try_pop() {
                    Ok(msg) => {
                        // The stream is not blocked on an event - drop the listener.
                        let this = self.as_mut().project();
                        *this.listener = None;
                        return Poll::Ready(Some(msg));
                    }
                    Err(PopError::Closed) => {
                        // The stream is not blocked on an event - drop the listener.
                        let this = self.as_mut().project();
                        *this.listener = None;
                        return Poll::Ready(None);
                    }
                    Err(PopError::Empty) => {}
                }

                // Receiving failed - now start listening for notifications or wait for one.
                let this = self.as_mut().project();
                if this.listener.is_some() {
                    // Go back to the outer loop to wait for a notification.
                    break;
                } else {
                    *this.listener = Some(this.channel.stream_ops.listen());
                }
            }
        }
    }
}

impl<T> futures_core::stream::FusedStream for Receiver<T> {
    fn is_terminated(&self) -> bool {
        self.channel.queue.is_closed() && self.channel.queue.is_empty()
    }
}

/// A [`Sender`] that does not prevent the channel from being closed.
///
/// This is created through the [`Sender::downgrade`] method. In order to use it, it needs
/// to be upgraded into a [`Sender`] through the `upgrade` method.
pub struct WeakSender<T> {
    channel: Arc<Channel<T>>,
}

impl<T> WeakSender<T> {
    /// Upgrade the [`WeakSender`] into a [`Sender`].
    pub fn upgrade(&self) -> Option<Sender<T>> {
        if self.channel.queue.is_closed() {
            None
        } else {
            match self.channel.sender_count.fetch_update(
                Ordering::Relaxed,
                Ordering::Relaxed,
                |count| if count == 0 { None } else { Some(count + 1) },
            ) {
                Err(_) => None,
                Ok(new_value) if new_value > usize::MAX / 2 => {
                    // Make sure the count never overflows, even if lots of sender clones are leaked.
                    abort();
                }
                Ok(_) => Some(Sender {
                    channel: self.channel.clone(),
                }),
            }
        }
    }
}

impl<T> Clone for WeakSender<T> {
    fn clone(&self) -> Self {
        WeakSender {
            channel: self.channel.clone(),
        }
    }
}

impl<T> fmt::Debug for WeakSender<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "WeakSender {{ .. }}")
    }
}

/// A [`Receiver`] that does not prevent the channel from being closed.
///
/// This is created through the [`Receiver::downgrade`] method. In order to use it, it needs
/// to be upgraded into a [`Receiver`] through the `upgrade` method.
pub struct WeakReceiver<T> {
    channel: Arc<Channel<T>>,
}

impl<T> WeakReceiver<T> {
    /// Upgrade the [`WeakReceiver`] into a [`Receiver`].
    pub fn upgrade(&self) -> Option<Receiver<T>> {
        if self.channel.queue.is_closed() {
            None
        } else {
            match self.channel.receiver_count.fetch_update(
                Ordering::Relaxed,
                Ordering::Relaxed,
                |count| if count == 0 { None } else { Some(count + 1) },
            ) {
                Err(_) => None,
                Ok(new_value) if new_value > usize::MAX / 2 => {
                    // Make sure the count never overflows, even if lots of receiver clones are leaked.
                    abort();
                }
                Ok(_) => Some(Receiver {
                    channel: self.channel.clone(),
                    listener: None,
                    _pin: PhantomPinned,
                }),
            }
        }
    }
}

impl<T> Clone for WeakReceiver<T> {
    fn clone(&self) -> Self {
        WeakReceiver {
            channel: self.channel.clone(),
        }
    }
}

impl<T> fmt::Debug for WeakReceiver<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "WeakReceiver {{ .. }}")
    }
}

easy_wrapper! {
    /// A future returned by [`Sender::push()`].
    #[derive(Debug)]
    #[must_use = "futures do nothing unless you `.await` or poll them"]
    pub struct Push<'a, T>(PushInner<'a, T> => Result<(), PushError<T>>);
    #[cfg(always_disabled)]
    pub(self) wait();
}

pin_project! {
    #[derive(Debug)]
    #[project(!Unpin)]
    struct PushInner<'a, T> {
        // Reference to the original sender.
        sender: &'a Sender<T>,

        // The message to send.
        msg: Option<T>,

        // Listener waiting on the channel.
        listener: Option<EventListener>,

        // Keeping this type `!Unpin` enables future optimizations.
        #[pin]
        _pin: PhantomPinned
    }
}

impl<'a, T> EventListenerFuture for PushInner<'a, T> {
    type Output = Result<(), PushError<T>>;

    /// Run this future with the given `Strategy`.
    fn poll_with_strategy<'x, S: Strategy<'x>>(
        self: Pin<&mut Self>,
        strategy: &mut S,
        context: &mut S::Context,
    ) -> Poll<Result<(), PushError<T>>> {
        let this = self.project();

        loop {
            let msg = this.msg.take().unwrap();
            // Attempt to send a message.
            match this.sender.try_push(msg) {
                Ok(_) => return Poll::Ready(Ok(())),
                Err(PushError::Full(m)) => *this.msg = Some(m),
                Err(error) => return Poll::Ready(Err(error)),
            }

            // Sending failed - now start listening for notifications or wait for one.
            if this.listener.is_some() {
                // Poll using the given strategy
                ready!(S::poll(strategy, &mut *this.listener, context));
            } else {
                *this.listener = Some(this.sender.channel.send_ops.listen());
            }
        }
    }
}

easy_wrapper! {
    /// A future returned by [`Receiver::pop()`].
    #[derive(Debug)]
    #[must_use = "futures do nothing unless you `.await` or poll them"]
    pub struct Pop<'a, T>(PopInner<'a, T> => Result<T, PopError>);
    #[cfg(always_disabled)]
    pub(crate) wait();
}

pin_project! {
    #[derive(Debug)]
    #[project(!Unpin)]
    struct PopInner<'a, T> {
        // Reference to the receiver.
        receiver: &'a Receiver<T>,

        // Listener waiting on the channel.
        listener: Option<EventListener>,

        // Keeping this type `!Unpin` enables future optimizations.
        #[pin]
        _pin: PhantomPinned
    }
}

impl<'a, T> EventListenerFuture for PopInner<'a, T> {
    type Output = Result<T, PopError>;

    /// Run this future with the given `Strategy`.
    fn poll_with_strategy<'x, S: Strategy<'x>>(
        self: Pin<&mut Self>,
        strategy: &mut S,
        cx: &mut S::Context,
    ) -> Poll<Result<T, PopError>> {
        let this = self.project();

        loop {
            // Attempt to receive a message.
            match this.receiver.try_pop() {
                Ok(msg) => return Poll::Ready(Ok(msg)),
                Err(PopError::Empty) => {}
                Err(error) => return Poll::Ready(Err(error)),
            }

            // Receiving failed - now start listening for notifications or wait for one.
            if this.listener.is_some() {
                // Poll using the given strategy
                ready!(S::poll(strategy, &mut *this.listener, cx));
            } else {
                *this.listener = Some(this.receiver.channel.recv_ops.listen());
            }
        }
    }
}
