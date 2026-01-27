type Slot = usize;
const SLOT_BITS: usize = Slot::BITS as _;
const SLOT_BITS_U8: u8 = SLOT_BITS as u8;
const SLOT_COUNT: usize = 256 / SLOT_BITS;

#[derive(Clone, Copy, Debug, Default)]
pub struct Bitset([Slot; SLOT_COUNT]);

impl Bitset {
    pub fn is_empty(&self) -> bool {
        self.0.iter().copied().fold(0, |acc, slot| acc | slot) == 0
    }

    pub fn len(&self) -> u8 {
        self.0.iter().map(|slot| slot.count_ones() as u8).sum()
    }

    #[allow(dead_code)]
    pub fn get(&self, index: u8) -> bool {
        let slot = index / SLOT_BITS_U8;
        let shift = index % SLOT_BITS_U8;
        let flag = 1 << shift;
        self.slot(slot) & flag != 0
    }

    pub fn insert(&mut self, index: u8) {
        self.update(index, true)
    }

    pub fn remove(&mut self, index: u8) {
        self.update(index, false)
    }

    pub fn update(&mut self, index: u8, enabled: bool) {
        let slot = index / SLOT_BITS_U8;
        let shift = index % SLOT_BITS_U8;
        let flag = 1 << shift;
        let slot = self.slot_mut(slot);
        if enabled {
            *slot |= flag;
        } else {
            *slot &= !flag;
        }
    }

    pub fn next_occupied(&self, mut index: u8) -> Option<u8> {
        let mut slot_index = index / SLOT_BITS_U8;
        let mut shift = index % SLOT_BITS_U8;

        loop {
            let slot = self.slot(slot_index);

            let trailing = (slot >> shift).trailing_zeros() as u8;

            if trailing != SLOT_BITS_U8 {
                return Some(index + trailing);
            }

            index = index.checked_add(SLOT_BITS_U8 - shift)?;
            slot_index += 1;
            shift = 0;
        }
    }

    fn slot(&self, index: u8) -> &Slot {
        if cfg!(test) {
            debug_assert!(index < (SLOT_COUNT as u8));
        }
        unsafe { self.0.get_unchecked(index as usize) }
    }

    fn slot_mut(&mut self, index: u8) -> &mut Slot {
        if cfg!(test) {
            debug_assert!(index < (SLOT_COUNT as u8));
        }
        unsafe { self.0.get_unchecked_mut(index as usize) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::{collections::BTreeSet, vec::Vec};
    use bolero::{check, generator::*};

    #[derive(Clone, Copy, Debug, TypeGenerator)]
    enum Op {
        Insert(u8),
        Remove(u8),
    }

    #[test]
    fn differential_test() {
        let mut set = BTreeSet::new();
        check!().with_type::<Vec<Op>>().for_each(move |ops| {
            let mut bitset = Bitset::default();

            for op in ops.iter().copied() {
                match op {
                    Op::Insert(i) => {
                        bitset.insert(i);
                        set.insert(i);
                    }
                    Op::Remove(i) => {
                        bitset.remove(i);
                        set.remove(&i);
                    }
                }
            }

            for i in 0..=255 {
                assert_eq!(set.contains(&i), bitset.get(i));
            }

            set.clear();
        });
    }

    #[test]
    fn next_occupied_test() {
        // simple implementation
        fn next_occupied_simple(bitset: &Bitset, mut index: u8) -> Option<u8> {
            loop {
                if bitset.get(index) {
                    return Some(index);
                }
                index = index.checked_add(1)?;
            }
        }

        check!().with_type::<Vec<Op>>().for_each(move |ops| {
            let mut bitset = Bitset::default();

            for op in ops.iter().copied() {
                match op {
                    Op::Insert(i) => {
                        bitset.insert(i);
                    }
                    Op::Remove(i) => {
                        bitset.remove(i);
                    }
                }
            }

            // make sure the simple implementation matches the optimized one
            for index in 0..=255 {
                assert_eq!(
                    next_occupied_simple(&bitset, index),
                    bitset.next_occupied(index),
                    "index: {index}"
                );
            }
        });
    }
}
