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

    pub fn push_number(&mut self, number: f32) {
        self.number.push(number);
    }

    pub fn get_vector(&self, register: VectorRegister) -> Vector {
        self.vector[register.0]
    }

    pub fn push_vector(&mut self, vector: Vector) {
        self.vector.push(vector);
    }

    pub fn get_rgb(&self, register: RgbRegister) -> LinSrgba {
        self.rgb[register.0]
    }

    pub fn push_rgb(&mut self, rgb: LinSrgba) {
        self.rgb.push(rgb);
    }

    pub fn clear(&mut self) {
        self.number.clear();
        self.vector.clear();
        self.rgb.clear();
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
