use crate::project::{
    expressions::BinaryOperator,
    spectra::SpectrumId,
    textures::{ColorTextureId, MonoTextureId},
};

use super::{
    registers::{NumberRegister, RgbRegister, VectorRegister},
    Inputs,
};

#[derive(Copy, Clone)]
pub(super) struct Instruction<N, V> {
    pub instruction_type: InstructionType<N, V>,
    pub dependencies: Inputs,
}

#[derive(Copy, Clone)]
pub(super) enum InstructionType<N, V> {
    NumberValue {
        number: f32,
        output: NumberRegister,
    },
    VectorValue {
        x: NumberValue<N>,
        y: NumberValue<N>,
        z: NumberValue<N>,
        w: NumberValue<N>,
        output: VectorRegister,
    },
    RgbValue {
        red: NumberValue<N>,
        green: NumberValue<N>,
        blue: NumberValue<N>,
        output: RgbRegister,
    },
    SpectrumValue {
        wavelength: NumberValue<N>,
        spectrum: SpectrumId,
        output: NumberRegister,
    },
    ColorTextureValue {
        texture_coordinates: VectorValue<V>,
        texture: ColorTextureId,
        output: RgbRegister,
    },
    MonoTextureValue {
        texture_coordinates: VectorValue<V>,
        texture: MonoTextureId,
        output: NumberRegister,
    },
    RgbSpectrumValue {
        wavelength: NumberValue<N>,
        source: RgbRegister,
        output: NumberRegister,
    },
    Fresnel {
        ior: NumberValue<N>,
        env_ior: NumberValue<N>,
        normal: VectorValue<V>,
        incident: VectorValue<V>,
        output: NumberRegister,
    },
    Blackbody {
        wavelength: NumberValue<N>,
        temperature: NumberValue<N>,
        output: NumberRegister,
    },
    Convert {
        conversion: ValueConversion,
    },
    Binary {
        value_type: BinaryValueType,
        operator: BinaryOperator,
        lhs: usize,
        rhs: usize,
        output: usize,
    },
    Mix {
        value_type: BinaryValueType,
        lhs: usize,
        rhs: usize,
        amount: NumberValue<N>,
        output: usize,
    },
    Clamp {
        value: NumberValue<N>,
        min: NumberValue<N>,
        max: NumberValue<N>,
        output: NumberRegister,
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
    RgbToVector {
        source: RgbRegister,
        output: VectorRegister,
    },
}

#[derive(Copy, Clone)]
pub(super) enum BinaryValueType {
    Number,
    Vector,
    Rgb,
}
