use crossbeam::atomic::AtomicCell;

use crate::{
    light::LightPool, materials::DispersedOutput, pooling::SlicePool, program::ExecutionContext,
    renderer::samplers::Sampler,
};

pub(crate) struct Tools<'r, 'a> {
    pub sampler: &'r mut dyn Sampler,
    pub light_pool: &'r LightPool<'r>,
    pub interaction_output_pool: &'r SlicePool<'r, DispersedOutput, crate::utils::RefCell>,
    pub execution_context: &'r mut ExecutionContext<'a>,
}

#[repr(transparent)]
pub(crate) struct AtomicF32(AtomicCell<u32>);

impl AtomicF32 {
    pub(crate) fn new(value: f32) -> Self {
        Self(AtomicCell::new(value.to_bits()))
    }

    pub(crate) fn load(&self) -> f32 {
        f32::from_bits(self.0.load())
    }

    pub(crate) fn store(&self, value: f32) {
        self.0.store(value.to_bits());
    }

    pub(crate) fn add_assign(&self, value: f32) {
        let mut currant_bits = self.0.load();
        let mut attempts = 0;

        // Discard the value if multiple threads are stuck updating it
        while attempts < 5 {
            let result = self.0.compare_exchange(
                currant_bits,
                (f32::from_bits(currant_bits) + value).to_bits(),
            );

            if let Err(current) = result {
                currant_bits = current;
                attempts += 1;
            } else {
                break;
            }
        }
    }
}

pub(crate) trait Locking<T> {
    type Locked: BorrowMut<Item = T>;
    fn lock(item: T) -> Self::Locked;
}

pub(crate) enum Mutex {}

impl<T> Locking<T> for Mutex {
    type Locked = parking_lot::Mutex<T>;

    fn lock(item: T) -> Self::Locked {
        parking_lot::Mutex::new(item)
    }
}

pub(crate) enum RefCell {}

impl<T> Locking<T> for RefCell {
    type Locked = std::cell::RefCell<T>;

    fn lock(item: T) -> Self::Locked {
        std::cell::RefCell::new(item)
    }
}

pub(crate) trait BorrowMut {
    type Item;

    fn borrow_mut<T>(&self, use_item: impl FnOnce(&mut Self::Item) -> T) -> T;
}

impl<T> BorrowMut for parking_lot::Mutex<T> {
    type Item = T;

    fn borrow_mut<U>(&self, use_item: impl FnOnce(&mut Self::Item) -> U) -> U {
        use_item(&mut self.lock())
    }
}

impl<T> BorrowMut for std::cell::RefCell<T> {
    type Item = T;

    fn borrow_mut<U>(&self, use_item: impl FnOnce(&mut Self::Item) -> U) -> U {
        use_item(&mut self.borrow_mut())
    }
}
