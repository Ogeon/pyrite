use std::{convert::TryFrom, error::Error, path::PathBuf};

use palette::{LinSrgb, Srgb};

use crate::{
    math::utils::Interpolated,
    project::{ComplexExpression, FromComplexExpression},
    texture::Texture,
    tracer::{ParametricValue, RenderContext},
};

pub enum Color {
    Spectrum(Interpolated),
    BuiltinSpectrum(Interpolated<&'static [(f32, f32)]>),
    Rgb(LinSrgb),
    Constant(f32),
    Texture(Texture),
}

impl ParametricValue<RenderContext, f32> for Color {
    fn get(&self, context: &RenderContext) -> f32 {
        match self {
            Color::Spectrum(interpolated) => interpolated.get(context.wavelength),
            Color::BuiltinSpectrum(interpolated) => interpolated.get(context.wavelength),
            &Color::Rgb(LinSrgb {
                red, green, blue, ..
            }) => {
                let wavelength = context.wavelength;

                let red_response = red * crate::rgb::response::RED.get(wavelength);
                let green_response = green * crate::rgb::response::GREEN.get(wavelength);
                let blue_response = blue * crate::rgb::response::BLUE.get(wavelength);

                red_response + green_response + blue_response
            }
            Color::Constant(constant) => *constant,
            Color::Texture(texture) => {
                let position = context.texture;
                let LinSrgb {
                    red, green, blue, ..
                } = texture.get_color(position).color;
                let wavelength = context.wavelength;

                let red_response = red * crate::rgb::response::RED.get(wavelength);
                let green_response = green * crate::rgb::response::GREEN.get(wavelength);
                let blue_response = blue * crate::rgb::response::BLUE.get(wavelength);

                red_response + green_response + blue_response
            }
        }
    }
}

impl From<f32> for Color {
    fn from(constant: f32) -> Self {
        Color::Constant(constant)
    }
}

impl FromComplexExpression for Color {
    fn from_complex_expression(
        value: ComplexExpression,
        make_path: &impl Fn(&str) -> PathBuf,
    ) -> Result<Self, Box<dyn Error>> {
        match value {
            ComplexExpression::Vector { .. } => Err("vector cannot be used as color".into()),
            ComplexExpression::Fresnel { .. } => Err("Fresnel cannot be used as color".into()),
            ComplexExpression::LightSource { name } => match &*name {
                "d65" => Ok(Color::BuiltinSpectrum(Interpolated {
                    points: crate::light_source::D65,
                })),
                _ => Err(format!("unknown light source: '{}'", name).into()),
            },
            ComplexExpression::Spectrum { points } => {
                let points = Vec::try_from(points)?;
                Ok(Color::Spectrum(Interpolated {
                    points: points
                        .into_iter()
                        .map(|pair| <(_, _)>::try_from(pair).map(|(w, i)| (w as f32, i as f32)))
                        .collect::<Result<_, _>>()?,
                }))
            }
            ComplexExpression::Rgb { red, green, blue } => Ok(Color::Rgb(
                Srgb::new(
                    red.parse(make_path)?,
                    green.parse(make_path)?,
                    blue.parse(make_path)?,
                )
                .into_linear(),
            )),
            ComplexExpression::Texture { path } => {
                Ok(Color::Texture(Texture::from_path(make_path(&path))?))
            }
            ComplexExpression::Add { .. }
            | ComplexExpression::Sub { .. }
            | ComplexExpression::Mul { .. }
            | ComplexExpression::Div { .. }
            | ComplexExpression::Mix { .. } => Err("unexpected unhandled operator as color".into()),
        }
    }
}
