use crate::project::{
    expressions::BinaryOperator,
    spectra::SpectrumId,
    textures::{ColorTextureId, MonoTextureId},
};

use super::registers::{NumberRegister, RgbRegister, VectorRegister};

#[derive(Copy, Clone)]
pub(super) enum Instruction<N, V> {
    NumberInput {
        input_value: N,
    },
    VectorInput {
        input_value: V,
    },
    NumberValue {
        number: f32,
    },
    VectorValue {
        x: NumberValue,
        y: NumberValue,
        z: NumberValue,
        w: NumberValue,
    },
    RgbValue {
        red: NumberValue,
        green: NumberValue,
        blue: NumberValue,
    },
    SpectrumValue {
        wavelength: NumberValue,
        spectrum: SpectrumId,
    },
    ColorTextureValue {
        texture_coordinates: VectorRegister,
        texture: ColorTextureId,
    },
    MonoTextureValue {
        texture_coordinates: VectorRegister,
        texture: MonoTextureId,
    },
    RgbSpectrumValue {
        wavelength: NumberValue,
        source: RgbRegister,
    },
    Fresnel {
        ior: NumberValue,
        env_ior: NumberValue,
        normal: VectorRegister,
        incident: VectorRegister,
    },
    Blackbody {
        wavelength: NumberValue,
        temperature: NumberValue,
    },
    Convert {
        conversion: ValueConversion,
    },
    Binary {
        value_type: BinaryValueType,
        operator: BinaryOperator,
        lhs: usize,
        rhs: usize,
    },
    Mix {
        value_type: BinaryValueType,
        lhs: usize,
        rhs: usize,
        amount: NumberValue,
    },
    Clamp {
        value: NumberValue,
        min: NumberValue,
        max: NumberValue,
    },
}

#[derive(Copy, Clone)]
pub(super) enum NumberValue {
    Constant(f32),
    Register(NumberRegister),
}

#[derive(Copy, Clone)]
pub(super) enum ValueConversion {
    RgbToVector { source: RgbRegister },
}

#[derive(Copy, Clone)]
pub(super) enum BinaryValueType {
    Number,
    Vector,
    Rgb,
}
