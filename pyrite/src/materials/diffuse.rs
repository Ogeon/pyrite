use cgmath::{Point2, Vector3};

use super::{CoherentOutput, DispersedOutput, InteractionOutput, SurfaceInteraction};
use crate::{
    light::{CoherentLight, DispersedLight, Wavelengths},
    math::{same_hemisphere, utils::sample_cosine_hemisphere},
    tracer::{LightProgram, RenderContext},
    utils::Tools,
};

pub(super) fn sample_reflection_coherent<'t, 'a>(
    out_direction: Vector3<f32>,
    texture_coordinate: Point2<f32>,
    color: LightProgram<'a>,
    wavelengths: &Wavelengths,
    tools: &mut Tools<'t, 'a>,
) -> SurfaceInteraction<InteractionOutput<'t>> {
    let in_direction = if out_direction.z < 0.0 {
        -sample_cosine_hemisphere(tools.sampler)
    } else {
        sample_cosine_hemisphere(tools.sampler)
    };

    let mut reflectivity = tools.light_pool.get();

    let initial_input = RenderContext {
        wavelength: wavelengths.hero(),
        normal: Vector3::unit_z(),
        ray_direction: -out_direction,
        texture: texture_coordinate,
    };

    let mut color_program = color.memoize(initial_input, tools.execution_context);

    for (bin, wavelength) in reflectivity.iter_mut().zip(wavelengths) {
        color_program.update_input().set_wavelength(wavelength);
        *bin = color_program.run() * std::f32::consts::FRAC_1_PI;
    }

    SurfaceInteraction {
        diffuse: true,
        glossy: false,
        output: InteractionOutput::Coherent(CoherentOutput {
            in_direction,
            pdf: pdf(out_direction, in_direction),
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
    let in_direction = if out_direction.z < 0.0 {
        -sample_cosine_hemisphere(tools.sampler)
    } else {
        sample_cosine_hemisphere(tools.sampler)
    };

    let input = RenderContext {
        wavelength: wavelengths[wavelength_index],
        normal: Vector3::unit_z(),
        ray_direction: -out_direction,
        texture: texture_coordinate,
    };

    SurfaceInteraction {
        diffuse: true,
        glossy: false,
        output: DispersedOutput {
            in_direction,
            pdf: pdf(out_direction, in_direction),
            reflectivity: DispersedLight::new(
                wavelength_index,
                tools.execution_context.run(color, &input) * std::f32::consts::FRAC_1_PI,
            ),
        },
    }
}

pub(super) fn evaluate_coherent<'t, 'a>(
    out_direction: Vector3<f32>,
    texture_coordinate: Point2<f32>,
    color: LightProgram<'a>,
    wavelengths: &Wavelengths,
    tools: &mut Tools<'t, 'a>,
) -> CoherentLight<'t> {
    let mut reflectivity = tools.light_pool.get();

    let initial_input = RenderContext {
        wavelength: wavelengths.hero(),
        normal: Vector3::unit_z(),
        ray_direction: -out_direction,
        texture: texture_coordinate,
    };

    let mut color_program = color.memoize(initial_input, tools.execution_context);

    for (bin, wavelength) in reflectivity.iter_mut().zip(wavelengths) {
        color_program.update_input().set_wavelength(wavelength);
        *bin = color_program.run() * std::f32::consts::FRAC_1_PI;
    }

    reflectivity
}

pub(super) fn evaluate_dispersed<'t, 'a>(
    out_direction: Vector3<f32>,
    texture_coordinate: Point2<f32>,
    color: LightProgram<'a>,
    wavelength_index: usize,
    wavelengths: &Wavelengths,
    tools: &mut Tools<'t, 'a>,
) -> DispersedLight {
    let input = RenderContext {
        wavelength: wavelengths[wavelength_index],
        normal: Vector3::unit_z(),
        ray_direction: -out_direction,
        texture: texture_coordinate,
    };

    DispersedLight::new(
        wavelength_index,
        tools.execution_context.run(color, &input) * std::f32::consts::FRAC_1_PI,
    )
}

pub(super) fn pdf(out_direction: Vector3<f32>, in_direction: Vector3<f32>) -> f32 {
    if same_hemisphere(out_direction, in_direction) {
        in_direction.z.abs() * std::f32::consts::FRAC_1_PI
    } else {
        0.0
    }
}
