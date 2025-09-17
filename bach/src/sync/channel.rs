use crate::{
    coop::Operation,
    queue::{CloseError, PopError, PushError, Pushable},
    sync::queue::Shared as Queue,
};
use alloc::sync::Arc;
use core::{
    fmt,
    future::Future,
    marker::{PhantomData, PhantomPinned},
    pin::{pin, Pin},
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
use std::{
    process::abort,
    task::{RawWaker, RawWakerVTable, Waker},
};

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

impl<T, Q> Channel<T, Q> {
    fn notify_after_send(&self) {
        // Notify a blocked receive operation. If the notified operation gets canceled,
        // it will notify another blocked receive operation.
        self.recv_ops.notify_additional(1);

        // Notify all blocked streams.
        self.stream_ops.notify(usize::MAX);
    }

    fn notify_after_recv(&self) {
        // Notify a blocked send operation. If the notified operation gets canceled, it
        // will notify another blocked send operation.
        self.send_ops.notify_additional(1);
    }
}

/// Creates a channel.
pub fn new<T, Q>(queue: Q) -> (Sender<T>, Receiver<T>)
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

    let sender_waker = waker::<_, _, true>(&channel);
    let receiver_waker = waker::<_, _, false>(&channel);

    let channel: Arc<Channel<T>> = channel;

    let sender = Sender {
        channel: channel.clone(),
        waker: sender_waker,
        listener: None,
    };

    let receiver = Receiver {
        channel: channel.clone(),
        waker: receiver_waker,
        listener: None,
        _pin: PhantomPinned,
    };

    (sender, receiver)
}

fn waker<Q, T, const IS_SEND: bool>(channel: &Arc<Channel<T, Q>>) -> Waker {
    use core::mem::ManuallyDrop;

    #[inline(always)]
    unsafe fn clone_waker<T, Q, const IS_SEND: bool>(waker: *const ()) -> RawWaker {
        unsafe { Arc::increment_strong_count(waker as *const Channel<T, Q>) };
        RawWaker::new(
            waker,
            &RawWakerVTable::new(
                clone_waker::<T, Q, IS_SEND>,
                wake::<T, Q, IS_SEND>,
                wake_by_ref::<T, Q, IS_SEND>,
                drop_waker::<T, Q, IS_SEND>,
            ),
        )
    }

    unsafe fn wake<T, Q, const IS_SEND: bool>(waker: *const ()) {
        let channel = unsafe { Arc::from_raw(waker as *const Channel<T, Q>) };
        if IS_SEND {
            channel.notify_after_send();
        } else {
            channel.notify_after_recv();
        }
    }

    unsafe fn wake_by_ref<T, Q, const IS_SEND: bool>(waker: *const ()) {
        let channel = unsafe { ManuallyDrop::new(Arc::from_raw(waker as *const Channel<T, Q>)) };
        if IS_SEND {
            channel.notify_after_send();
        } else {
            channel.notify_after_recv();
        }
    }

    unsafe fn drop_waker<T, Q, const IS_SEND: bool>(waker: *const ()) {
        unsafe { Arc::decrement_strong_count(waker as *const Channel<T, Q>) };
    }

    unsafe {
        let ptr = Arc::into_raw(channel.clone()) as *const _;
        let raw = RawWaker::new(
            ptr,
            &RawWakerVTable::new(
                clone_waker::<T, Q, IS_SEND>,
                wake::<T, Q, IS_SEND>,
                wake_by_ref::<T, Q, IS_SEND>,
                drop_waker::<T, Q, IS_SEND>,
            ),
        );
        Waker::from_raw(raw)
    }
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

    // Listens for a send or close event to unblock this stream.
    listener: Option<EventListener>,

    waker: Waker,
}

impl<T> Sender<T> {
    /// Pushes a message into the channel, waiting for capacity to become available
    pub async fn push(&mut self, msg: T) -> Result<(), PushError> {
        self.channel.send_resource.acquire().await;

        let mut msg = Some(msg);

        Push::_new(PushInner {
            sender: self,
            msg: &mut msg,
            _pin: PhantomPinned,
        })
        .await
    }

    /// Pushes a message into the channel, but doesn't wait for capacity to become available
    pub async fn push_nowait(&mut self, msg: T) -> Result<Option<T>, PushError> {
        self.channel.send_resource.acquire().await;

        let mut msg = Some(msg);
        match self.push_unchecked(&mut msg) {
            Ok(prev) => Ok(prev),
            Err(PushError::Full) => Ok(msg.take()),
            Err(PushError::Closed) => Err(PushError::Closed),
        }
    }

    pub fn poll_push<P: Pushable<T>>(
        &mut self,
        cx: &mut Context,
        msg: &mut P,
    ) -> Poll<Result<(), PushError>> {
        ready!(self.channel.send_resource.poll_acquire(cx));

        let p = Push::_new(PushInner {
            sender: self,
            msg,
            _pin: PhantomPinned,
        });
        let p = pin!(p);
        p.poll(cx)
    }

    pub async fn send(&mut self, msg: T) -> Result<(), PushError> {
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
            waker: self.waker.clone(),
        }
    }

    /// Returns whether the senders belong to the same channel.
    pub fn same_channel(&self, other: &Sender<T>) -> bool {
        Arc::ptr_eq(&self.channel, &other.channel)
    }

    #[inline]
    fn push_unchecked(&self, msg: &mut dyn Pushable<T>) -> Result<Option<T>, PushError> {
        let mut ctx = core::task::Context::from_waker(&self.waker);
        self.channel.queue.push_with_notify(msg, &mut ctx)
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
            waker: self.waker.clone(),
            listener: None,
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
        waker: Waker,

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
    /// Pops a message from the channel.
    pub async fn pop(&mut self) -> Result<T, PopError> {
        self.channel.recv_resource.acquire().await;

        Pop::_new(PopInner {
            receiver: self,
            _pin: PhantomPinned,
        })
        .await
    }

    pub fn poll_pop(&mut self, cx: &mut Context) -> Poll<Result<T, PopError>> {
        ready!(self.channel.recv_resource.poll_acquire(cx));

        let p = Pop::_new(PopInner {
            receiver: self,
            _pin: PhantomPinned,
        });
        let p = pin!(p);
        p.poll(cx)
    }

    pub async fn recv(&mut self) -> Result<T, PopError> {
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
            waker: self.waker.clone(),
        }
    }

    /// Returns whether the receivers belong to the same channel.
    pub fn same_channel(&self, other: &Receiver<T>) -> bool {
        Arc::ptr_eq(&self.channel, &other.channel)
    }

    #[inline]
    fn pop_unchecked(&self) -> Result<T, PopError> {
        let mut ctx = core::task::Context::from_waker(&self.waker);
        self.channel.queue.pop_with_notify(&mut ctx)
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
            waker: self.waker.clone(),
            listener: None,
            _pin: PhantomPinned,
        }
    }
}

impl<T> Stream for Receiver<T> {
    type Item = T;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        ready!(self.channel.recv_resource.poll_acquire(cx));

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
                match self.pop_unchecked() {
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
    waker: Waker,
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
                    waker: self.waker.clone(),
                    listener: None,
                }),
            }
        }
    }
}

impl<T> Clone for WeakSender<T> {
    fn clone(&self) -> Self {
        WeakSender {
            channel: self.channel.clone(),
            waker: self.waker.clone(),
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
    waker: Waker,
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
                    waker: self.waker.clone(),
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
            waker: self.waker.clone(),
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
    #[must_use = "futures do nothing unless you `.await` or poll them"]
    pub struct Push<'a, T, P: Pushable<T>>(PushInner<'a, T, P> => Result<(), PushError>);
    #[cfg(always_disabled)]
    pub(self) wait();
}

pin_project! {
    #[project(!Unpin)]
    struct PushInner<'a, T, P> {
        // Reference to the original sender.
        sender: &'a mut Sender<T>,

        // The message to send.
        msg: &'a mut P,

        // Keeping this type `!Unpin` enables future optimizations.
        #[pin]
        _pin: PhantomPinned
    }
}

impl<T, P: Pushable<T>> EventListenerFuture for PushInner<'_, T, P> {
    type Output = Result<(), PushError>;

    /// Run this future with the given `Strategy`.
    fn poll_with_strategy<'x, S: Strategy<'x>>(
        self: Pin<&mut Self>,
        strategy: &mut S,
        context: &mut S::Context,
    ) -> Poll<Result<(), PushError>> {
        let this = self.project();

        loop {
            // Attempt to send a message.
            match this.sender.push_unchecked(*this.msg) {
                Ok(_) => return Poll::Ready(Ok(())),
                Err(PushError::Full) => {}
                Err(PushError::Closed) => return Poll::Ready(Err(PushError::Closed)),
            }

            // Sending failed - now start listening for notifications or wait for one.
            if this.sender.listener.is_some() {
                // Poll using the given strategy
                ready!(S::poll(strategy, &mut this.sender.listener, context));
            } else {
                this.sender.listener = Some(this.sender.channel.send_ops.listen());
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
        receiver: &'a mut Receiver<T>,

        // Keeping this type `!Unpin` enables future optimizations.
        #[pin]
        _pin: PhantomPinned
    }
}

impl<T> EventListenerFuture for PopInner<'_, T> {
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
            match this.receiver.pop_unchecked() {
                Ok(msg) => return Poll::Ready(Ok(msg)),
                Err(PopError::Empty) => {}
                Err(error) => return Poll::Ready(Err(error)),
            }

            // Receiving failed - now start listening for notifications or wait for one.
            if this.receiver.listener.is_some() {
                // Poll using the given strategy
                ready!(S::poll(strategy, &mut this.receiver.listener, cx));
            } else {
                this.receiver.listener = Some(this.receiver.channel.recv_ops.listen());
            }
        }
    }
}
