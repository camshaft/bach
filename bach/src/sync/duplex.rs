use super::{
    channel::{self, Receiver, Sender},
    queue::Queue,
};

pub struct Duplex<S, R = S> {
    pub sender: Sender<S>,
    pub receiver: Receiver<R>,
}

impl<S, R> Duplex<S, R> {
    pub fn pair<AtoB, BtoA>(a_to_b: AtoB, b_to_a: BtoA) -> (Self, Duplex<R, S>)
    where
        AtoB: Queue<S> + 'static + Send + Sync,
        BtoA: Queue<R> + 'static + Send + Sync,
    {
        let (a_sender, b_receiver) = channel::new(a_to_b);
        let (b_sender, a_receiver) = channel::new(b_to_a);

        let a = Duplex {
            sender: a_sender,
            receiver: a_receiver,
        };

        let b = Duplex {
            sender: b_sender,
            receiver: b_receiver,
        };

        (a, b)
    }
}
