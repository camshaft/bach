use super::WakerId;
use core::task::Waker;
use std::collections::{btree_map::Entry, BTreeMap};

type Idx = u32;
type Rank = u32;

#[derive(Clone, Debug, Default)]
pub struct DisjointSet {
    /// A mapping of operation IDs to internal indices
    operation_to_idx: BTreeMap<u64, Idx>,
    /// A mapping of waker `data` pointers to internal indices
    waker_to_idx: BTreeMap<WakerId, Idx>,
    /// A cached mapping of root indices to lists of wakers that belong to that
    /// root
    root_to_result_id: BTreeMap<Idx, Vec<Waker>>,
    /// A cached list of lists of wakers. This avoids doing short lived allocations.
    cache: Vec<Vec<Waker>>,
    inner: Inner,
}

impl DisjointSet {
    /// Joins a waker to an operation
    ///
    /// This will merge any other wakers that are also interested in that operation
    /// into the same scheduling group.
    #[inline]
    pub fn join(&mut self, waker: &Waker, waker_id: WakerId, operation: u64) {
        // insert the operation first so it has a smaller index, which will result
        // in a more shallow tree
        let op_idx = match self.operation_to_idx.entry(operation) {
            Entry::Occupied(entry) => *entry.get(),
            Entry::Vacant(entry) => {
                let idx = self.inner.insert(None);
                entry.insert(idx);
                idx
            }
        };

        let actor_idx = match self.waker_to_idx.entry(waker_id) {
            Entry::Occupied(entry) => *entry.get(),
            Entry::Vacant(entry) => {
                let idx = self.inner.insert(Some(waker.clone()));
                entry.insert(idx);
                idx
            }
        };

        self.inner.join(op_idx, actor_idx);
    }

    /// The maximum group size
    #[inline]
    pub fn max_group_size(&self) -> Rank {
        self.inner.max_group_size
    }

    /// Returns the number of tasks that were woken
    #[inline]
    pub fn schedule<F: FnMut(&mut Vec<Waker>)>(&mut self, mut schedule_group: F) -> usize {
        let tasks = self.waker_to_idx.len();

        self.operation_to_idx.clear();
        self.waker_to_idx.clear();

        self.inner.drain(|root, waker| {
            self.root_to_result_id
                .entry(root)
                .or_insert_with(|| self.cache.pop().unwrap_or_else(|| Vec::with_capacity(2)))
                .push(waker);
        });

        self.root_to_result_id.retain(|_root_idx, wakers| {
            // waker lists only need to be created for groups with more than 1 member
            if cfg!(test) {
                assert!(wakers.len() >= 2);
            }

            // let the caller schedule the group
            schedule_group(wakers);

            // clear out the list before putting it back into the cache
            wakers.clear();
            self.cache.push(core::mem::take(wakers));

            // clear out the mapping
            false
        });

        unsafe {
            // SAFETY: all of the wakers have been cleared so we can avoid `drop_in_place` call
            if cfg!(test) {
                for slot in self.inner.slots.iter() {
                    assert!(slot.waker.is_none());
                }
            }

            self.inner.slots.set_len(0);
        }

        tasks
    }

    #[cfg(test)]
    fn sets(&mut self) -> Vec<Vec<Rank>> {
        let mut result = Vec::new();
        let mut root_to_result_id = BTreeMap::new();

        for index in 0..self.inner.len() {
            let root = self.inner.find_root(index);

            let result_id = *root_to_result_id.entry(root).or_insert_with(|| {
                let id = result.len();
                result.push(Vec::with_capacity(1));
                id
            });

            if self.inner.slot(index).waker.is_some() {
                result[result_id].push(index);
            }
        }

        result
    }
}

#[derive(Clone, Debug, Default)]
struct Inner {
    slots: Vec<Slot>,
    max_group_size: Rank,
}

#[derive(Debug, Clone)]
struct Slot {
    parent: Rank,
    group_size: Rank,
    waker: Option<Waker>,
}

impl Slot {
    #[inline]
    fn wake(&mut self) {
        if let Some(waker) = self.waker.take() {
            waker.wake();
        }
    }
}

impl Inner {
    #[inline]
    fn len(&self) -> u32 {
        self.slots.len() as u32
    }

    #[inline]
    fn insert(&mut self, waker: Option<Waker>) -> Idx {
        let idx = self.slots.len() as _;
        let group_size = waker.is_some().into();
        let slot = Slot {
            parent: idx,
            group_size,
            waker,
        };
        self.slots.push(slot);
        idx
    }

    #[inline]
    fn join(&mut self, x: Idx, y: Idx) {
        let x = self.find_root(x);
        let y = self.find_root(y);

        // they're already in the same set
        if x == y {
            return;
        }

        let x_slot = self.slot(x);
        let y_slot = self.slot(y);

        let waker_depth = x_slot.group_size + y_slot.group_size;
        self.max_group_size = self.max_group_size.max(waker_depth);

        // prefer smaller indices as parents
        if x < y {
            self.slot_mut(y).parent = x;
            self.slot_mut(x).group_size = waker_depth;
        } else {
            self.slot_mut(x).parent = y;
            self.slot_mut(y).group_size = waker_depth;
        }
    }

    #[inline]
    fn drain<F: FnMut(Rank, Waker)>(&mut self, mut on_waker: F) {
        let max_group_size = core::mem::take(&mut self.max_group_size);

        // if there are only 1-member groups, then wake everything in one go
        if max_group_size < 2 {
            for mut slot in self.slots.drain(..) {
                slot.wake();
            }
            return;
        }

        // iterate from the beginning index to preserve insertion order
        for idx in 0..self.len() {
            let Some(waker) = self.slot_mut(idx).waker.take() else {
                // this is an operation node so keep iterating
                continue;
            };

            let root = self.find_root(idx);

            // if we only have a single waker for the group then no need
            // to push to a list and interleave scheduling
            if self.slot(root).group_size == 1 {
                waker.wake();
                continue;
            }

            on_waker(root, waker);
        }
    }

    #[inline]
    fn find_root(&mut self, x: Idx) -> Idx {
        macro_rules! parent {
            ($x:expr) => {
                self.slot_mut($x).parent
            };
        }

        let mut curr = x;

        loop {
            // compress paths as we are searching
            let parent = parent!(curr);

            if curr == parent {
                break;
            }

            // compress the tree by making the grandparent a parent
            let grandparent = parent!(parent);
            parent!(curr) = grandparent;

            curr = grandparent;
        }

        curr
    }

    #[inline(always)]
    fn slot(&self, idx: Idx) -> &Slot {
        if cfg!(test) {
            return &self.slots[idx as usize];
        }
        unsafe { self.slots.get_unchecked(idx as usize) }
    }

    #[inline(always)]
    fn slot_mut(&mut self, idx: Idx) -> &mut Slot {
        if cfg!(test) {
            return &mut self.slots[idx as usize];
        }
        unsafe { self.slots.get_unchecked_mut(idx as usize) }
    }
}

#[cfg(test)]
mod tests {
    use super::{DisjointSet, Rank};
    use bolero::{check, TypeGenerator};
    use std::{
        collections::{BTreeMap, VecDeque},
        sync::{Arc, Mutex},
        task::{Wake, Waker},
    };

    #[derive(TypeGenerator, Clone, Debug)]
    struct Model {
        joins: Vec<(u16, u16)>,
    }

    struct QueueWaker {
        queue: Arc<Mutex<VecDeque<u16>>>,
        id: u16,
    }

    impl Wake for QueueWaker {
        fn wake(self: Arc<Self>) {
            self.queue.lock().unwrap().push_back(self.id);
        }
    }

    impl Model {
        fn run(&self) -> VecDeque<u16> {
            let mut set = DisjointSet::default();

            let queue = Arc::new(Mutex::new(VecDeque::new()));
            let mut wakers = BTreeMap::new();

            for (waker_id, operation) in self.joins.iter().copied() {
                let waker = wakers.entry(waker_id).or_insert_with(|| {
                    Waker::from(Arc::new(QueueWaker {
                        queue: queue.clone(),
                        id: waker_id,
                    }))
                });

                set.join(waker, (waker_id as _, 0), operation as u64);
            }

            let sets = set.sets();

            let actual_max_depth = sets.iter().map(|set| set.len() as Rank).max().unwrap_or(0);

            let max_depth = set.max_group_size();
            assert_eq!(actual_max_depth, max_depth);

            set.schedule(|wakers| {
                assert!((1..=max_depth as usize).contains(&wakers.len()));
                for waker in wakers.drain(..) {
                    waker.wake();
                }
            });

            let mut queue = queue.lock().unwrap();
            // TODO do some checks on these interleavings
            core::mem::take(&mut *queue)
        }
    }

    #[test]
    fn model_test() {
        check!()
            .with_type::<Model>()
            .with_test_time(core::time::Duration::from_secs(10))
            .for_each(|model| {
                model.run();
            });
    }

    #[test]
    fn two_group_test() {
        Model {
            joins: vec![(0, 10), (1, 10), (2, 20), (3, 20), (4, 20)],
        }
        .run();
    }

    #[test]
    fn join_group_test() {
        Model {
            joins: vec![(0, 10), (0, 20), (1, 10), (2, 20)],
        }
        .run();
    }
}
