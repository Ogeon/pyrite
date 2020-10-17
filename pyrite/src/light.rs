use std::{cell::RefCell, fmt::Debug, ops::AddAssign, ops::Div, ops::Mul, ops::MulAssign};

use bumpalo::Bump;

use crate::{film::Film, renderer::samplers::Sampler};

pub(crate) struct Light<'a> {
    single_wavelength: bool,
    bins: Option<&'a mut [f32]>,
    pool: &'a LightPool<'a>,
}

impl<'a> Light<'a> {
    pub(crate) fn is_black(&self) -> bool {
        for bin in self {
            if *bin > 0.0 {
                return false;
            }
        }

        true
    }

    pub(crate) fn set_all(&mut self, value: f32) {
        for bin in self {
            *bin = value;
        }
    }

    pub(crate) fn set_single_wavelength(&mut self) {
        self.single_wavelength = true;
    }

    pub(crate) fn max(&self) -> f32 {
        self.iter()
            .fold(0.0, |previous, &value| previous.max(value))
    }

    pub(crate) fn iter(&self) -> <&Self as IntoIterator>::IntoIter {
        self.into_iter()
    }

    pub(crate) fn iter_mut(&mut self) -> <&mut Self as IntoIterator>::IntoIter {
        self.into_iter()
    }
}

impl<'a> AddAssign for Light<'a> {
    fn add_assign(&mut self, rhs: Self) {
        self.single_wavelength = self.single_wavelength || rhs.single_wavelength;

        for (lhs, rhs) in self.iter_mut().zip(&rhs) {
            *lhs += rhs;
        }
    }
}

impl<'a> MulAssign for Light<'a> {
    fn mul_assign(&mut self, rhs: Self) {
        self.single_wavelength = self.single_wavelength || rhs.single_wavelength;

        for (lhs, rhs) in self.iter_mut().zip(&rhs) {
            *lhs *= rhs;
        }
    }
}

impl<'a> MulAssign<f32> for Light<'a> {
    fn mul_assign(&mut self, rhs: f32) {
        for lhs in self {
            *lhs *= rhs;
        }
    }
}

impl<'a> Mul for Light<'a> {
    type Output = Self;

    fn mul(mut self, rhs: Self) -> Self {
        self.single_wavelength = self.single_wavelength || rhs.single_wavelength;

        for (lhs, rhs) in self.iter_mut().zip(&rhs) {
            *lhs *= rhs;
        }

        self
    }
}

impl<'a> Mul<&'_ Self> for Light<'a> {
    type Output = Self;

    fn mul(mut self, rhs: &'_ Self) -> Self {
        self.single_wavelength = self.single_wavelength || rhs.single_wavelength;

        for (lhs, rhs) in self.iter_mut().zip(rhs) {
            *lhs *= rhs;
        }

        self
    }
}

impl<'a> Mul<f32> for Light<'a> {
    type Output = Self;

    fn mul(mut self, rhs: f32) -> Self {
        for lhs in &mut self {
            *lhs *= rhs;
        }

        self
    }
}

impl<'a> Div<f32> for Light<'a> {
    type Output = Self;

    fn div(mut self, rhs: f32) -> Self {
        for lhs in &mut self {
            *lhs /= rhs;
        }

        self
    }
}

impl<'a> Drop for Light<'a> {
    fn drop(&mut self) {
        if let Some(bins) = self.bins.take() {
            self.pool.recycle(bins);
        }
    }
}

impl<'r, 'a> IntoIterator for &'r Light<'a> {
    type Item = &'r f32;

    type IntoIter = std::iter::Take<std::slice::Iter<'r, f32>>;

    fn into_iter(self) -> Self::IntoIter {
        let amount = if self.single_wavelength {
            1
        } else {
            self.bins.as_ref().unwrap().len()
        };

        self.bins.as_ref().unwrap().iter().take(amount)
    }
}

impl<'r, 'a> IntoIterator for &'r mut Light<'a> {
    type Item = &'r mut f32;

    type IntoIter = std::iter::Take<std::slice::IterMut<'r, f32>>;

    fn into_iter(self) -> Self::IntoIter {
        let amount = if self.single_wavelength {
            1
        } else {
            self.bins.as_ref().unwrap().len()
        };

        self.bins.as_mut().unwrap().iter_mut().take(amount)
    }
}

impl<'a> Debug for Light<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut dbg_struct = f.debug_struct("Light");
        dbg_struct.field("single_wavelength", &self.single_wavelength);
        dbg_struct.field("bins", &self.bins);
        dbg_struct.finish()
    }
}

pub(crate) struct LightPool<'a> {
    arena: &'a Bump,
    pool: RefCell<Vec<&'a mut [f32]>>,
    bins: usize,
}

impl<'a> LightPool<'a> {
    pub(crate) fn new(arena: &'a Bump, bins: usize) -> LightPool<'a> {
        let pool = std::iter::repeat_with(|| arena.alloc_slice_fill_copy(bins, 0.0))
            .take(4)
            .collect();

        LightPool {
            arena,
            pool: RefCell::new(pool),
            bins,
        }
    }

    #[inline(always)]
    pub(crate) fn get(&'a self) -> Light<'a> {
        self.with_value(0.0)
    }

    #[inline(always)]
    pub(crate) fn with_value(&'a self, value: f32) -> Light<'a> {
        let bins = if let Some(bins) = self.pool.borrow_mut().pop() {
            for bin in &mut *bins {
                *bin = value;
            }

            bins
        } else {
            self.new_bins(value)
        };

        Light {
            single_wavelength: false,
            bins: Some(bins),
            pool: self,
        }
    }

    #[inline(always)]
    fn new_bins(&self, value: f32) -> &'a mut [f32] {
        self.arena.alloc_slice_fill_copy(self.bins, value)
    }

    #[inline(always)]
    fn recycle(&self, bins: &'a mut [f32]) {
        self.pool.borrow_mut().push(bins);
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
