use std::usize;

use cgmath::{Point2, Vector3};

use crate::{
    light::{DispersedLight, Wavelengths},
    tracer::LightProgram,
    tracer::RenderContext,
    utils::Tools,
};

use super::{CoherentOutput, DispersedOutput, InteractionOutput, SurfaceInteraction};

pub(super) fn sample_reflection_coherent<'t, 'a>(
    out_direction: Vector3<f32>,
    texture_coordinate: Point2<f32>,
    color: LightProgram<'a>,
    wavelengths: &Wavelengths,
    tools: &mut Tools<'t, 'a>,
) -> SurfaceInteraction<InteractionOutput<'t>> {
    let in_direction = Vector3::new(-out_direction.x, -out_direction.y, out_direction.z);

    let mut reflectivity = tools.light_pool.get();

    if in_direction.z != 0.0 {
        let initial_input = RenderContext {
            wavelength: wavelengths.hero(),
            normal: Vector3::unit_z(),
            ray_direction: -out_direction,
            texture: texture_coordinate,
        };

        let mut color_program = color.memoize(initial_input, tools.execution_context);

        let abs_cos_in = in_direction.z.abs();

        if abs_cos_in > 0.0 {
            for (bin, wavelength) in reflectivity.iter_mut().zip(wavelengths) {
                color_program.update_input().set_wavelength(wavelength);
                *bin = color_program.run() / abs_cos_in;
            }
        }
    }

    SurfaceInteraction {
        diffuse: false,
        glossy: false,
        output: InteractionOutput::Coherent(CoherentOutput {
            in_direction,
            pdf: if in_direction.z == 0.0 { 0.0 } else { 1.0 },
            reflectivity,
        }),
    }
}

pub(super) fn sample_reflection_dispersed<'t, 'a>(
    out_direction: Vector3<f32>,
    texture_coordinate: Point2<f32>,
    color: LightProgram<'a>,
    wavelength_index: usize,
    wavelengths: &Wavelengths,
    tools: &mut Tools<'t, 'a>,
) -> SurfaceInteraction<DispersedOutput> {
    let in_direction = Vector3::new(-out_direction.x, -out_direction.y, out_direction.z);

    let reflectivity = if in_direction.z != 0.0 {
        let input = RenderContext {
            wavelength: wavelengths[wavelength_index],
            normal: Vector3::unit_z(),
            ray_direction: -out_direction,
            texture: texture_coordinate,
        };

        tools.execution_context.run(color, &input) / in_direction.z.abs()
    } else {
        0.0
    };

    SurfaceInteraction {
        diffuse: false,
        glossy: false,
        output: DispersedOutput {
            in_direction,
            pdf: if in_direction.z == 0.0 { 0.0 } else { 1.0 },
            reflectivity: DispersedLight::new(wavelength_index, reflectivity),
        },
    }
}
