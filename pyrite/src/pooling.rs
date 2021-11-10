use std::ops::{Deref, DerefMut};

use bumpalo::Bump;

use crate::utils::{BorrowMut, Locking};

pub(crate) type PooledSlice<'a, T, L = crate::utils::RefCell> =
    Pooled<'a, <L as Locking<Vec<&'a mut [T]>>>::Locked>;

pub(crate) trait Pool<'a> {
    type Item: ?Sized;

    fn recycle_item(&self, item: &'a mut Self::Item);
}

pub(crate) struct Pooled<'a, P: Pool<'a>> {
    item: Option<&'a mut P::Item>,
    pool: &'a P,
}

impl<'a, P: Pool<'a>> std::fmt::Debug for Pooled<'a, P>
where
    P::Item: std::fmt::Debug,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.item.fmt(f)
    }
}

impl<'a, P: Pool<'a>> Pooled<'a, P> {
    pub fn new(item: &'a mut P::Item, pool: &'a P) -> Self {
        Self {
            item: Option::Some(item),
            pool,
        }
    }
}

impl<'a, 'r, P: Pool<'a>> IntoIterator for &'r Pooled<'a, P>
where
    &'r P::Item: IntoIterator,
{
    type Item = <&'r P::Item as IntoIterator>::Item;

    type IntoIter = <&'r P::Item as IntoIterator>::IntoIter;

    fn into_iter(self) -> Self::IntoIter {
        (&**self).into_iter()
    }
}

impl<'a, 'r, P: Pool<'a>> IntoIterator for &'r mut Pooled<'a, P>
where
    &'r mut P::Item: IntoIterator,
{
    type Item = <&'r mut P::Item as IntoIterator>::Item;

    type IntoIter = <&'r mut P::Item as IntoIterator>::IntoIter;

    fn into_iter(self) -> Self::IntoIter {
        (&mut **self).into_iter()
    }
}

impl<'a, P: Pool<'a>> Deref for Pooled<'a, P> {
    type Target = P::Item;

    #[track_caller]
    fn deref(&self) -> &Self::Target {
        self.item.as_ref().expect("using recycled value")
    }
}

impl<'a, P: Pool<'a>> DerefMut for Pooled<'a, P> {
    #[track_caller]
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.item.as_mut().expect("using recycled value")
    }
}

impl<'a, P: Pool<'a>> Drop for Pooled<'a, P> {
    fn drop(&mut self) {
        if let Some(item) = self.item.take() {
            self.pool.recycle_item(item);
        }
    }
}

pub(crate) struct SlicePool<'a, T, L, A = Bump>
where
    L: Locking<Vec<&'a mut [T]>>,
{
    length: usize,
    arena: &'a A,
    pool: L::Locked,
}

impl<'a, T, L, A> SlicePool<'a, T, L, A>
where
    L: Locking<Vec<&'a mut [T]>>,
    A: Arena<T>,
{
    pub fn new(arena: &'a A, length: usize) -> Self {
        Self {
            length,
            arena,
            pool: L::lock(Vec::new()),
        }
    }

    pub fn with_capacity_fill_copy(
        arena: &'a A,
        length: usize,
        capacity: usize,
        default_item: T,
    ) -> Self
    where
        T: Copy,
    {
        let pool = L::lock(
            std::iter::repeat_with(|| arena.alloc_slice_fill_copy(length, default_item))
                .take(capacity)
                .collect(),
        );

        Self {
            length,
            arena,
            pool,
        }
    }

    pub fn get_fill_copy(&'a self, value: T) -> Pooled<'a, L::Locked>
    where
        T: Copy,
    {
        let slice = if let Some(slice) = self.pool.borrow_mut(|pool| pool.pop()) {
            for item in &mut *slice {
                *item = value
            }

            slice
        } else {
            self.arena.alloc_slice_fill_copy(self.length, value)
        };

        Pooled::new(slice, &self.pool)
    }

    pub fn get_fill_iter<I>(&'a self, iterator: I) -> Pooled<'a, L::Locked>
    where
        I: IntoIterator<Item = T>,
        I::IntoIter: ExactSizeIterator,
    {
        let mut iterator = iterator.into_iter();
        assert!(iterator.len() == self.length);

        let slice = if let Some(slice) = self.pool.borrow_mut(|pool| pool.pop()) {
            for (item, value) in slice.iter_mut().zip(iterator) {
                *item = value
            }

            slice
        } else {
            self.arena.alloc_slice_fill_iter(&mut iterator)
        };

        Pooled::new(slice, &self.pool)
    }
}

impl<'a, T, P> Pool<'a> for P
where
    P: BorrowMut<Item = Vec<&'a mut T>>,
    T: ?Sized + 'a,
{
    type Item = T;

    fn recycle_item(&self, item: &'a mut Self::Item) {
        self.borrow_mut(move |pool| pool.push(item));
    }
}

pub(crate) trait Arena<T> {
    fn alloc_slice_fill_copy(&self, length: usize, default: T) -> &mut [T]
    where
        T: Copy;
    fn alloc_slice_fill_iter(&self, iterator: &mut dyn ExactSizeIterator<Item = T>) -> &mut [T];
}

impl<T> Arena<T> for Bump {
    fn alloc_slice_fill_copy(&self, length: usize, default: T) -> &mut [T]
    where
        T: Copy,
    {
        self.alloc_slice_fill_copy(length, default)
    }

    fn alloc_slice_fill_iter(&self, iterator: &mut dyn ExactSizeIterator<Item = T>) -> &mut [T] {
        self.alloc_slice_fill_iter(iterator)
    }
}

impl<'a, T, A: Arena<T>> Arena<T> for &'a A {
    fn alloc_slice_fill_copy(&self, length: usize, default: T) -> &mut [T]
    where
        T: Copy,
    {
        (*self).alloc_slice_fill_copy(length, default)
    }

    fn alloc_slice_fill_iter(&self, iterator: &mut dyn ExactSizeIterator<Item = T>) -> &mut [T] {
        (*self).alloc_slice_fill_iter(iterator)
    }
}

impl<T> Arena<T> for colosseum::sync::Arena<T> {
    fn alloc_slice_fill_copy(&self, length: usize, default: T) -> &mut [T]
    where
        T: Copy,
    {
        self.alloc_extend(std::iter::repeat(default).take(length))
    }

    fn alloc_slice_fill_iter(&self, iterator: &mut dyn ExactSizeIterator<Item = T>) -> &mut [T] {
        self.alloc_extend(iterator)
    }
}
