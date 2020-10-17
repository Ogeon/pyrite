use cgmath::{Point2, Vector3};

use crate::{
    program::{Inputs, MemoizedInput, NumberInput, ProgramFor, ProgramInput, VectorInput},
    project::expressions::Vector,
};

use std::{borrow::Cow, convert::TryFrom};

pub(crate) type LightProgram<'p> = ProgramFor<'p, RenderContext, f32>;

pub trait ParametricValue<From, To>: Send + Sync {
    fn get(&self, i: &From) -> To;
}

impl<From> ParametricValue<From, f32> for f32 {
    fn get(&self, _: &From) -> f32 {
        *self
    }
}

pub struct NormalInput {
    pub normal: Vector3<f32>,
    pub incident: Vector3<f32>,
    pub texture: Point2<f32>,
}

impl ProgramInput for NormalInput {
    type NumberInput = NormalNumberInput;
    type VectorInput = SurfaceVectorInput;

    #[inline(always)]
    fn get_number_input(&self, input: Self::NumberInput) -> f32 {
        match input {}
    }

    #[inline(always)]
    fn get_vector_input(&self, input: Self::VectorInput) -> Vector {
        match input {
            SurfaceVectorInput::Normal => self.normal.into(),
            SurfaceVectorInput::RayDirection => self.incident.into(),
            SurfaceVectorInput::TextureCoordinates => self.texture.into(),
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) enum NormalNumberInput {}

impl TryFrom<NumberInput> for NormalNumberInput {
    type Error = Cow<'static, str>;

    fn try_from(value: NumberInput) -> Result<Self, Self::Error> {
        match value {
            NumberInput::Wavelength => {
                Err("the wavelength is not available during normal mapping".into())
            }
        }
    }
}

pub struct RenderContext {
    pub wavelength: f32,
    pub normal: Vector3<f32>,
    pub ray_direction: Vector3<f32>,
    pub texture: Point2<f32>,
}

impl ProgramInput for RenderContext {
    type NumberInput = RenderNumberInput;
    type VectorInput = SurfaceVectorInput;

    #[inline(always)]
    fn get_number_input(&self, input: Self::NumberInput) -> f32 {
        match input {
            RenderNumberInput::Wavelength => self.wavelength,
        }
    }

    #[inline(always)]
    fn get_vector_input(&self, input: Self::VectorInput) -> Vector {
        match input {
            SurfaceVectorInput::Normal => self.normal.into(),
            SurfaceVectorInput::RayDirection => self.ray_direction.into(),
            SurfaceVectorInput::TextureCoordinates => self.texture.into(),
        }
    }
}

impl<'a> MemoizedInput<'a> for RenderContext {
    type Updater = RenderContextUpdater<'a>;

    fn new_updater(&'a mut self, changes: &'a mut Inputs) -> Self::Updater {
        RenderContextUpdater {
            input: self,
            changes,
        }
    }
}

pub(crate) struct RenderContextUpdater<'a> {
    input: &'a mut RenderContext,
    changes: &'a mut Inputs,
}

impl<'a> RenderContextUpdater<'a> {
    pub fn set_wavelength(&mut self, wavelength: f32) {
        self.input.wavelength = wavelength;
        self.changes.insert(Inputs::WAVELENGTH);
    }
}

#[derive(Clone, Copy)]
pub(crate) enum RenderNumberInput {
    Wavelength,
}

impl TryFrom<NumberInput> for RenderNumberInput {
    type Error = Cow<'static, str>;

    fn try_from(value: NumberInput) -> Result<Self, Self::Error> {
        match value {
            NumberInput::Wavelength => Ok(RenderNumberInput::Wavelength),
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) enum SurfaceVectorInput {
    Normal,
    RayDirection,
    TextureCoordinates,
}

impl TryFrom<VectorInput> for SurfaceVectorInput {
    type Error = Cow<'static, str>;

    fn try_from(value: VectorInput) -> Result<Self, Self::Error> {
        match value {
            VectorInput::Normal => Ok(SurfaceVectorInput::Normal),
            VectorInput::RayDirection => Ok(SurfaceVectorInput::RayDirection),
            VectorInput::TextureCoordinates => Ok(SurfaceVectorInput::TextureCoordinates),
        }
    }
}

#[derive(Clone)]
pub struct Light {
    wavelength: f32,
    white: bool,
}
