use std::{borrow::Cow, convert::TryFrom, error::Error};

use crate::project::{expressions::Vector, spectra::Spectra, textures::Textures};

use instruction::Instruction;
use registers::{NumberRegister, VectorRegister};

pub(crate) use compiler::ProgramCompiler;
pub(crate) use execution_context::ExecutionContext;

mod compiler;
mod execution_context;
mod instruction;
mod registers;

pub(crate) type ProgramFor<'p, I, T> =
    Program<'p, <I as ProgramInput>::NumberInput, <I as ProgramInput>::VectorInput, T>;

pub(crate) struct Program<'p, N, V, T> {
    program_type: ProgramType<'p, N, V, T>,
}

impl<'p, N: Copy, V: Copy, T> Clone for Program<'p, N, V, T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<'p, N: Copy, V: Copy, T> Copy for Program<'p, N, V, T> {}

enum ProgramType<'p, N, V, T> {
    Constant {
        value: f32,
        convert: fn(f32) -> T,
    },
    Instructions {
        instructions: &'p [Instruction<N, V>],
        output: ProgramOutput<T>,
    },
}

impl<'p, N: Copy, V: Copy, T> Clone for ProgramType<'p, N, V, T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<'p, N: Copy, V: Copy, T> Copy for ProgramType<'p, N, V, T> {}

pub(crate) trait FromValue: Sized {
    fn from_number() -> Result<fn(f32) -> Self, Box<dyn Error>>;
    fn from_value() -> ProgramOutputType<Self>;
}

impl FromValue for f32 {
    fn from_number() -> Result<fn(f32) -> Self, Box<dyn Error>> {
        Ok(|number| number)
    }

    fn from_value() -> ProgramOutputType<Self> {
        ProgramOutputType::Number(|number| number)
    }
}

pub(crate) enum ProgramOutputType<T> {
    Number(fn(f32) -> T),
    Vector(fn(Vector) -> T),
}

enum ProgramOutput<T> {
    FromNumber(NumberRegister, fn(f32) -> T),
    FromVector(VectorRegister, fn(Vector) -> T),
}

impl<T> Clone for ProgramOutput<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for ProgramOutput<T> {}

pub(crate) trait ProgramInput {
    type NumberInput: TryFrom<NumberInput, Error = Cow<'static, str>> + Copy;
    type VectorInput: TryFrom<VectorInput, Error = Cow<'static, str>> + Copy;

    fn get_number_input(&self, input: Self::NumberInput) -> f32;
    fn get_vector_input(&self, input: Self::VectorInput) -> Vector;
}

#[derive(Hash, Eq, PartialEq, Copy, Clone)]
pub(crate) enum NumberInput {
    Wavelength,
}

#[derive(Hash, Eq, PartialEq, Copy, Clone)]
pub(crate) enum VectorInput {
    Normal,
    Incident,
    TextureCoordinates,
}

#[derive(Copy, Clone)]
pub(crate) struct Resources<'a> {
    pub spectra: &'a Spectra,
    pub textures: &'a Textures,
}
