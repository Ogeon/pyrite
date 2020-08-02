use crate::project::{
    expressions::BinaryOperator,
    spectra::SpectrumId,
    textures::{ColorTextureId, MonoTextureId},
};

use super::registers::{NumberRegister, RgbRegister};

#[derive(Copy, Clone)]
pub(super) enum Instruction<N, V> {
    NumberValue {
        number: f32,
    },
    VectorValue {
        x: NumberValue<N>,
        y: NumberValue<N>,
        z: NumberValue<N>,
        w: NumberValue<N>,
    },
    RgbValue {
        red: NumberValue<N>,
        green: NumberValue<N>,
        blue: NumberValue<N>,
    },
    SpectrumValue {
        wavelength: NumberValue<N>,
        spectrum: SpectrumId,
    },
    ColorTextureValue {
        texture_coordinates: VectorValue<V>,
        texture: ColorTextureId,
    },
    MonoTextureValue {
        texture_coordinates: VectorValue<V>,
        texture: MonoTextureId,
    },
    RgbSpectrumValue {
        wavelength: NumberValue<N>,
        source: RgbRegister,
    },
    Fresnel {
        ior: NumberValue<N>,
        env_ior: NumberValue<N>,
        normal: VectorValue<V>,
        incident: VectorValue<V>,
    },
    Blackbody {
        wavelength: NumberValue<N>,
        temperature: NumberValue<N>,
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
        amount: NumberValue<N>,
    },
    Clamp {
        value: NumberValue<N>,
        min: NumberValue<N>,
        max: NumberValue<N>,
    },
}

#[derive(Copy, Clone)]
pub(super) enum NumberValue<N> {
    Constant(f32),
    Input(N),
    Register(NumberRegister),
}

#[derive(Copy, Clone)]
pub(super) enum VectorValue<V> {
    Input(V),
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
