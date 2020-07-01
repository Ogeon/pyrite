use std::error::Error;

use palette::{LinSrgb, Srgb};

use crate::{
    math::utils::Interpolated,
    project::{
        expressions::Vector,
        program::{ProgramFn, ProgramValue},
    },
    tracer::RenderContext,
};

#[derive(Copy, Clone)]
#[repr(transparent)]
pub struct Light {
    pub value: f32,
}

impl ProgramValue<RenderContext> for Light {
    fn from_number(number: f32) -> Result<Self, Box<dyn Error>> {
        Ok(Light { value: number })
    }

    fn from_vector(_x: f32, _y: f32, _z: f32, _w: f32) -> Result<Self, Box<dyn Error>> {
        Err("vectors cannot be used as light".into())
    }

    fn number() -> Result<Option<ProgramFn<RenderContext, Self>>, Box<dyn Error>> {
        Ok(None)
    }

    fn vector() -> Result<Option<ProgramFn<RenderContext, Self>>, Box<dyn Error>> {
        Err("vectors cannot be used as light".into())
    }

    fn rgb() -> Result<Option<ProgramFn<RenderContext, Self>>, Box<dyn Error>> {
        Ok(Some(|registers, input, _| {
            let blue: f32 = registers.pop();
            let green: f32 = registers.pop();
            let red: f32 = registers.pop();

            let LinSrgb {
                red, green, blue, ..
            } = Srgb::new(red, green, blue).into_linear();

            let red_response = red * crate::rgb::response::RED.get(input.wavelength);
            let green_response = green * crate::rgb::response::GREEN.get(input.wavelength);
            let blue_response = blue * crate::rgb::response::BLUE.get(input.wavelength);

            Light {
                value: red_response + green_response + blue_response,
            }
        }))
    }

    fn spectrum() -> Result<Option<ProgramFn<RenderContext, Self>>, Box<dyn Error>> {
        Ok(Some(|registers, input, resources| {
            let spectrum = Interpolated {
                points: resources.spectra.get(registers.pop()),
            };
            Light {
                value: spectrum.get(input.wavelength),
            }
        }))
    }

    fn texture() -> Result<Option<ProgramFn<RenderContext, Self>>, Box<dyn Error>> {
        Ok(Some(|registers, input, resources| {
            let texture = resources.textures.get(registers.pop());
            let uv: Vector = registers.pop();

            let LinSrgb {
                red, green, blue, ..
            } = texture.get_color(uv.into()).color;

            let red_response = red * crate::rgb::response::RED.get(input.wavelength);
            let green_response = green * crate::rgb::response::GREEN.get(input.wavelength);
            let blue_response = blue * crate::rgb::response::BLUE.get(input.wavelength);

            Light {
                value: red_response + green_response + blue_response,
            }
        }))
    }

    fn add() -> Result<ProgramFn<RenderContext, Self>, Box<dyn Error>> {
        Ok(|registers, _, _| {
            let rhs: Light = registers.pop();
            let lhs: Light = registers.pop();
            Light {
                value: lhs.value + rhs.value,
            }
        })
    }

    fn sub() -> Result<ProgramFn<RenderContext, Self>, Box<dyn Error>> {
        Ok(|registers, _, _| {
            let rhs: Light = registers.pop();
            let lhs: Light = registers.pop();
            Light {
                value: lhs.value - rhs.value,
            }
        })
    }

    fn mul() -> Result<ProgramFn<RenderContext, Self>, Box<dyn Error>> {
        Ok(|registers, _, _| {
            let rhs: Light = registers.pop();
            let lhs: Light = registers.pop();
            Light {
                value: lhs.value * rhs.value,
            }
        })
    }

    fn div() -> Result<ProgramFn<RenderContext, Self>, Box<dyn Error>> {
        Ok(|registers, _, _| {
            let rhs: Light = registers.pop();
            let lhs: Light = registers.pop();
            Light {
                value: lhs.value / rhs.value,
            }
        })
    }

    fn mix() -> Result<ProgramFn<RenderContext, Self>, Box<dyn Error>> {
        Ok(|registers, _, _| {
            let amount = registers.pop::<f32>().min(1.0).max(0.0);
            let rhs: f32 = registers.pop();
            let lhs: f32 = registers.pop();
            Light {
                value: lhs * (1.0 - amount) + rhs * amount,
            }
        })
    }
    fn fresnel() -> Result<ProgramFn<RenderContext, Self>, Box<dyn Error>> {
        Ok(|registers, _, _| {
            let incident: Vector = registers.pop();
            let normal: Vector = registers.pop();
            let env_ior: f32 = registers.pop();
            let ior: f32 = registers.pop();
            Light {
                value: crate::math::fresnel(ior, env_ior, normal.into(), incident.into()),
            }
        })
    }
}
