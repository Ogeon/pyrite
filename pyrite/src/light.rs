use std::{
    fmt::Debug,
    ops::AddAssign,
    ops::Div,
    ops::Mul,
    ops::{Deref, DerefMut, DivAssign, Index, MulAssign},
};

use bumpalo::Bump;

use crate::{
    film::Film,
    pooling::{Arena, PooledSlice, SlicePool},
    renderer::samplers::Sampler,
    utils::Locking,
};

pub(crate) struct LightPool<'a, L = crate::utils::RefCell, A = Bump>
where
    L: Locking<Vec<&'a mut [f32]>> + Locking<Bump>,
{
    pool: SlicePool<'a, f32, L, A>,
}

impl<'a, L, A> LightPool<'a, L, A>
where
    L: Locking<Vec<&'a mut [f32]>> + Locking<Bump>,
    A: Arena<f32>,
{
    pub(crate) fn new(arena: &'a A, bins: usize) -> LightPool<'a, L, A> {
        LightPool {
            pool: SlicePool::with_capacity_fill_copy(arena, bins, 4, 0.0),
        }
    }

    #[inline(always)]
    pub(crate) fn get(&'a self) -> CoherentLight<'a, L> {
        self.with_value(0.0)
    }

    #[inline(always)]
    pub(crate) fn with_value(&'a self, value: f32) -> CoherentLight<'a, L> {
        CoherentLight {
            bins: self.pool.get_fill_copy(value),
        }
    }

    pub(crate) fn copy_slice<'b, L2>(
        &'a self,
        slice: &CoherentLight<'b, L2>,
    ) -> CoherentLight<'a, L>
    where
        L2: Locking<Vec<&'b mut [f32]>> + Locking<Bump>,
    {
        CoherentLight {
            bins: self.pool.get_fill_iter(slice.bins.iter().copied()),
        }
    }
}

#[repr(transparent)]
pub(crate) struct Wavelengths(Vec<f32>);

impl Wavelengths {
    pub fn new(length: usize) -> Wavelengths {
        assert!(length > 0, "need at least one wavelength sample");
        Wavelengths(std::iter::repeat(0.0).take(length).collect())
    }

    pub fn sample(&mut self, film: &Film, sampler: &mut dyn Sampler) {
        let wavelengths = film.sample_many_wavelengths(sampler, self.0.len());
        for (slot, wavelength) in self.0.iter_mut().zip(wavelengths) {
            *slot = wavelength;
        }

        // Pick a hero wavelength
        let index = sampler.gen_index(self.0.len()).unwrap();
        self.0.swap(0, index);
    }

    pub fn hero(&self) -> f32 {
        self.0[0]
    }
}

impl<'a> IntoIterator for &'a Wavelengths {
    type Item = f32;

    type IntoIter = std::iter::Cloned<std::slice::Iter<'a, f32>>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter().cloned()
    }
}

impl Index<usize> for Wavelengths {
    type Output = f32;

    fn index(&self, index: usize) -> &Self::Output {
        &self.0[index]
    }
}

pub(crate) struct CoherentLight<'a, L = crate::utils::RefCell>
where
    L: Locking<Vec<&'a mut [f32]>> + Locking<Bump>,
{
    bins: PooledSlice<'a, f32, L>,
}

impl<'a, L> Debug for CoherentLight<'a, L>
where
    L: Locking<Vec<&'a mut [f32]>> + Locking<Bump>,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.bins.fmt(f)
    }
}

impl<'a, L> CoherentLight<'a, L>
where
    L: Locking<Vec<&'a mut [f32]>> + Locking<Bump>,
{
    pub fn set_all(&mut self, value: f32) {
        for bin in &mut *self.bins {
            *bin = value;
        }
    }

    pub fn is_black(&self) -> bool {
        self.bins.iter().all(|&bin| bin == 0.0)
    }

    pub(crate) fn max(&self) -> f32 {
        self.iter()
            .fold(0.0, |previous, &value| previous.max(value))
    }

    pub fn disperse<'i>(&'i self) -> impl Iterator<Item = DispersedLight> + 'i {
        self.bins
            .iter()
            .enumerate()
            .map(|(index, &value)| DispersedLight { index, value })
    }
}

impl<'a, 'b, L1, L2> AddAssign<CoherentLight<'b, L2>> for CoherentLight<'a, L1>
where
    L1: Locking<Vec<&'a mut [f32]>> + Locking<Bump>,
    L2: Locking<Vec<&'b mut [f32]>> + Locking<Bump>,
{
    fn add_assign(&mut self, rhs: CoherentLight<'b, L2>) {
        for (lhs_bin, &rhs_bin) in self.bins.iter_mut().zip(&*rhs.bins) {
            *lhs_bin += rhs_bin;
        }
    }
}

impl<'a, L> MulAssign<f32> for CoherentLight<'a, L>
where
    L: Locking<Vec<&'a mut [f32]>> + Locking<Bump>,
{
    fn mul_assign(&mut self, rhs: f32) {
        for bin in &mut *self.bins {
            *bin *= rhs;
        }
    }
}

impl<'a, 'b, L1, L2> MulAssign<&'_ CoherentLight<'b, L2>> for CoherentLight<'a, L1>
where
    L1: Locking<Vec<&'a mut [f32]>> + Locking<Bump>,
    L2: Locking<Vec<&'b mut [f32]>> + Locking<Bump>,
{
    fn mul_assign(&mut self, rhs: &'_ CoherentLight<'b, L2>) {
        for (lhs_bin, &rhs_bin) in self.bins.iter_mut().zip(&*rhs.bins) {
            *lhs_bin *= rhs_bin;
        }
    }
}

impl<'a, 'b, L1, L2> MulAssign<CoherentLight<'b, L2>> for CoherentLight<'a, L1>
where
    L1: Locking<Vec<&'a mut [f32]>> + Locking<Bump>,
    L2: Locking<Vec<&'b mut [f32]>> + Locking<Bump>,
{
    fn mul_assign(&mut self, rhs: CoherentLight<'b, L2>) {
        *self *= &rhs;
    }
}

impl<'a, L> Mul<f32> for CoherentLight<'a, L>
where
    L: Locking<Vec<&'a mut [f32]>> + Locking<Bump>,
{
    type Output = Self;

    fn mul(mut self, rhs: f32) -> Self {
        self *= rhs;
        self
    }
}

impl<'a, 'b, L1, L2> Mul<CoherentLight<'b, L2>> for CoherentLight<'a, L1>
where
    L1: Locking<Vec<&'a mut [f32]>> + Locking<Bump>,
    L2: Locking<Vec<&'b mut [f32]>> + Locking<Bump>,
{
    type Output = Self;

    fn mul(mut self, rhs: CoherentLight<'b, L2>) -> Self {
        self *= rhs;
        self
    }
}

impl<'a, 'b, L1, L2> Mul<&'_ CoherentLight<'b, L2>> for CoherentLight<'a, L1>
where
    L1: Locking<Vec<&'a mut [f32]>> + Locking<Bump>,
    L2: Locking<Vec<&'b mut [f32]>> + Locking<Bump>,
{
    type Output = Self;

    fn mul(mut self, rhs: &'_ CoherentLight<'b, L2>) -> Self {
        self *= rhs;
        self
    }
}

impl<'a, L> DivAssign<f32> for CoherentLight<'a, L>
where
    L: Locking<Vec<&'a mut [f32]>> + Locking<Bump>,
{
    fn div_assign(&mut self, rhs: f32) {
        for lhs_bin in &mut *self.bins {
            *lhs_bin /= rhs;
        }
    }
}

impl<'a, 'b, L1, L2> DivAssign<CoherentLight<'b, L2>> for CoherentLight<'a, L1>
where
    L1: Locking<Vec<&'a mut [f32]>> + Locking<Bump>,
    L2: Locking<Vec<&'b mut [f32]>> + Locking<Bump>,
{
    fn div_assign(&mut self, rhs: CoherentLight<'b, L2>) {
        for (lhs_bin, &rhs_bin) in self.bins.iter_mut().zip(&*rhs.bins) {
            *lhs_bin /= rhs_bin;
        }
    }
}

impl<'a, L> Div<f32> for CoherentLight<'a, L>
where
    L: Locking<Vec<&'a mut [f32]>> + Locking<Bump>,
{
    type Output = Self;

    fn div(mut self, rhs: f32) -> Self {
        self /= rhs;
        self
    }
}

impl<'a, 'b, L1, L2> Div<CoherentLight<'b, L2>> for CoherentLight<'a, L1>
where
    L1: Locking<Vec<&'a mut [f32]>> + Locking<Bump>,
    L2: Locking<Vec<&'b mut [f32]>> + Locking<Bump>,
{
    type Output = Self;

    fn div(mut self, rhs: CoherentLight<'b, L2>) -> Self {
        self /= rhs;
        self
    }
}

impl<'a, L> Deref for CoherentLight<'a, L>
where
    L: Locking<Vec<&'a mut [f32]>> + Locking<Bump>,
{
    type Target = [f32];

    fn deref(&self) -> &Self::Target {
        &self.bins
    }
}

impl<'a, L> DerefMut for CoherentLight<'a, L>
where
    L: Locking<Vec<&'a mut [f32]>> + Locking<Bump>,
{
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.bins
    }
}

#[derive(Clone, Copy)]
pub(crate) struct DispersedLight {
    index: usize,
    value: f32,
}

impl DispersedLight {
    pub fn new(index: usize, value: f32) -> Self {
        Self { index, value }
    }

    pub fn set(&mut self, value: f32) {
        self.value = value;
    }

    pub fn value(&self) -> f32 {
        self.value
    }
}

impl AddAssign<DispersedLight> for DispersedLight {
    fn add_assign(&mut self, rhs: DispersedLight) {
        if self.index == rhs.index {
            self.value += rhs.value;
        }
    }
}

impl MulAssign<f32> for DispersedLight {
    fn mul_assign(&mut self, rhs: f32) {
        self.value *= rhs;
    }
}

impl Mul<f32> for DispersedLight {
    type Output = Self;

    fn mul(mut self, rhs: f32) -> Self::Output {
        self *= rhs;
        self
    }
}

impl DivAssign<f32> for DispersedLight {
    fn div_assign(&mut self, rhs: f32) {
        self.value /= rhs;
    }
}

impl Div<f32> for DispersedLight {
    type Output = Self;

    fn div(mut self, rhs: f32) -> Self::Output {
        self /= rhs;
        self
    }
}
