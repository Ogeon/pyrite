use palette::LinSrgba;

use crate::project::expressions::Vector;

pub(super) struct Registers {
    number: Vec<f32>,
    vector: Vec<Vector>,
    rgb: Vec<LinSrgba>,
}

impl Registers {
    pub fn new() -> Self {
        Registers {
            number: Vec::with_capacity(100),
            vector: Vec::with_capacity(100),
            rgb: Vec::with_capacity(100),
        }
    }

    pub fn get_number(&self, register: NumberRegister) -> f32 {
        self.number[register.0]
    }

    pub fn set_number(&mut self, number: f32, register: NumberRegister) {
        self.number[register.0] = number;
    }

    pub fn get_vector(&self, register: VectorRegister) -> Vector {
        self.vector[register.0]
    }

    pub fn set_vector(&mut self, vector: Vector, register: VectorRegister) {
        self.vector[register.0] = vector;
    }

    pub fn get_rgb(&self, register: RgbRegister) -> LinSrgba {
        self.rgb[register.0]
    }

    pub fn set_rgb(&mut self, rgb: LinSrgba, register: RgbRegister) {
        self.rgb[register.0] = rgb;
    }

    pub fn reserve(&mut self, numbers: usize, vectors: usize, rgb_values: usize) {
        self.number
            .resize_with(numbers.max(self.number.len()), Default::default);
        self.vector
            .resize_with(vectors.max(self.vector.len()), Default::default);
        self.rgb
            .resize_with(rgb_values.max(self.rgb.len()), Default::default);
    }
}

#[derive(Debug, Copy, Clone, Ord, PartialOrd, Eq, PartialEq, Hash)]
#[repr(transparent)]
pub(crate) struct NumberRegister(pub(super) usize);

#[derive(Debug, Copy, Clone, Ord, PartialOrd, Eq, PartialEq, Hash)]
#[repr(transparent)]
pub(crate) struct RgbRegister(pub(super) usize);

#[derive(Debug, Copy, Clone, Ord, PartialOrd, Eq, PartialEq, Hash)]
#[repr(transparent)]
pub(crate) struct VectorRegister(pub(super) usize);
