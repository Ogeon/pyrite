use rand::Rng;

pub(crate) trait Sampler: 'static {
    fn gen_f32(&mut self) -> f32;
}

impl dyn Sampler {
    pub(crate) fn gen<T: NewRandom>(&mut self) -> T {
        T::new_random(self)
    }

    pub(crate) fn gen_index(&mut self, length: usize) -> Option<usize> {
        if length == 0 {
            return None;
        }

        Some((length - 1).min((self.gen_f32() * length as f32) as usize))
    }

    pub(crate) fn select<'a, T>(&mut self, slice: &'a [T]) -> Option<&'a T> {
        self.gen_index(slice.len()).map(|index| &slice[index])
    }
}

impl<T: Rng + 'static> Sampler for T {
    fn gen_f32(&mut self) -> f32 {
        Rng::gen(self)
    }
}

pub(crate) trait NewRandom: Sized {
    fn new_random(sampler: &mut dyn Sampler) -> Self;
}

impl NewRandom for f32 {
    fn new_random(sampler: &mut dyn Sampler) -> Self {
        sampler.gen_f32()
    }
}
