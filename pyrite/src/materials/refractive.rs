use cgmath::{InnerSpace, Point2, Vector3};

use crate::{
    light::{DispersedLight, Wavelengths},
    math::face_forward,
    tracer::{LightProgram, RenderContext},
    utils::Tools,
};

use super::{CoherentOutput, DispersedOutput, InteractionOutput, SurfaceInteraction};

// Mostly based on the PBR book.
pub(super) fn sample_reflection_coherent<'t, 'a>(
    properties: &Properties,
    out_direction: Vector3<f32>,
    texture_coordinate: Point2<f32>,
    color: LightProgram<'a>,
    wavelengths: &Wavelengths,
    tools: &mut Tools<'t, 'a>,
) -> SurfaceInteraction<InteractionOutput<'t>> {
    let dispersed = properties.dispersion != 0.0 || properties.env_dispersion != 0.0;
    let refraction_threshold = tools.sampler.gen_f32();

    let initial_input = RenderContext {
        wavelength: wavelengths.hero(),
        normal: Vector3::unit_z(),
        ray_direction: -out_direction,
        texture: texture_coordinate,
    };
    let mut color_program = color.memoize(initial_input, tools.execution_context);

    if dispersed {
        let output = wavelengths
            .into_iter()
            .enumerate()
            .map(|(index, wavelength)| {
                let wl = wavelength * 0.001;
                let ior = properties.ior + properties.dispersion / (wl * wl);
                let env_ior = properties.env_ior + properties.env_dispersion / (wl * wl);

                let (in_direction, pdf) =
                    reflect_or_refract(refraction_threshold, out_direction, ior, env_ior);

                let abs_cos_in = in_direction.z.abs();
                let reflectivity = if abs_cos_in > 0.0 {
                    color_program.update_input().set_wavelength(wavelength);
                    pdf * color_program.run() / abs_cos_in
                } else {
                    0.0
                };

                DispersedOutput {
                    in_direction,
                    pdf: if in_direction.z == 0.0 { 0.0 } else { pdf },
                    reflectivity: DispersedLight::new(index, reflectivity),
                }
            });

        SurfaceInteraction {
            diffuse: false,
            glossy: false,
            output: InteractionOutput::Dispersed(
                tools.interaction_output_pool.get_fill_iter(output),
            ),
        }
    } else {
        let ior = properties.ior;
        let env_ior = properties.env_ior;

        let (in_direction, pdf) =
            reflect_or_refract(refraction_threshold, out_direction, ior, env_ior);

        let mut reflectivity = tools.light_pool.get();
        let abs_cos_in = in_direction.z.abs();
        if abs_cos_in > 0.0 {
            for (bin, wavelength) in reflectivity.iter_mut().zip(wavelengths) {
                color_program.update_input().set_wavelength(wavelength);
                *bin = pdf * color_program.run() / abs_cos_in;
            }
        }

        SurfaceInteraction {
            diffuse: false,
            glossy: false,
            output: InteractionOutput::Coherent(CoherentOutput {
                reflectivity,
                pdf: if in_direction.z == 0.0 { 0.0 } else { pdf },
                in_direction,
            }),
        }
    }
}

// Mostly based on the PBR book.
pub(super) fn sample_reflection_dispersed<'t, 'a>(
    properties: &Properties,
    out_direction: Vector3<f32>,
    texture_coordinate: Point2<f32>,
    color: LightProgram<'a>,
    wavelength_index: usize,
    wavelengths: &Wavelengths,
    tools: &mut Tools<'t, 'a>,
) -> SurfaceInteraction<DispersedOutput> {
    let dispersed = properties.dispersion != 0.0 || properties.env_dispersion != 0.0;
    let refraction_threshold = tools.sampler.gen_f32();

    let input = RenderContext {
        wavelength: wavelengths[wavelength_index],
        normal: Vector3::unit_z(),
        ray_direction: -out_direction,
        texture: texture_coordinate,
    };

    let (ior, env_ior) = if dispersed {
        let wl = wavelengths[wavelength_index] * 0.001;
        let ior = properties.ior + properties.dispersion / (wl * wl);
        let env_ior = properties.env_ior + properties.env_dispersion / (wl * wl);
        (ior, env_ior)
    } else {
        let ior = properties.ior;
        let env_ior = properties.env_ior;
        (ior, env_ior)
    };

    let (in_direction, pdf) = reflect_or_refract(refraction_threshold, out_direction, ior, env_ior);

    let abs_cos_in = in_direction.z.abs();
    let reflectivity = if abs_cos_in > 0.0 {
        pdf * tools.execution_context.run(color, &input) / abs_cos_in
    } else {
        0.0
    };

    SurfaceInteraction {
        diffuse: false,
        glossy: false,
        output: DispersedOutput {
            in_direction,
            pdf: if in_direction.z == 0.0 { 0.0 } else { pdf },
            reflectivity: DispersedLight::new(wavelength_index, reflectivity),
        },
    }
}

fn reflect_or_refract(
    refraction_threshold: f32,
    out_direction: Vector3<f32>,
    ior: f32,
    env_ior: f32,
) -> (Vector3<f32>, f32) {
    let fresnel = fresnel_dielectric(out_direction.z, env_ior, ior);

    if refraction_threshold < fresnel {
        let in_direction = Vector3::new(-out_direction.x, -out_direction.y, out_direction.z);

        (in_direction, fresnel)
    } else {
        let entering = out_direction.z > 0.0;
        let (ior, env_ior) = if entering {
            (ior, env_ior)
        } else {
            (env_ior, ior)
        };

        let in_direction = if let Some(in_direction) = refract(
            out_direction,
            face_forward(Vector3::unit_z(), out_direction),
            env_ior / ior,
        ) {
            in_direction
        } else {
            unreachable!();
        };

        (in_direction, 1.0 - fresnel)
    }
}

fn fresnel_dielectric(cos_theta_i: f32, env_ior: f32, ior: f32) -> f32 {
    let cos_theta_i = cos_theta_i.max(-1.0).min(1.0);

    // Potentially swap indices of refraction
    let entering = cos_theta_i > 0.0;
    let (cos_theta_i, eta_i, eta_t) = if entering {
        (cos_theta_i, env_ior, ior)
    } else {
        (cos_theta_i.abs(), ior, env_ior)
    };

    // Compute _cos_theta_t_ using Snell's law
    let sin_theta_i = (1.0 - cos_theta_i * cos_theta_i).max(0.0).sqrt();
    let sin_theta_i = eta_i / eta_t * sin_theta_i;

    // Handle total internal reflection
    if sin_theta_i >= 1.0 {
        1.0
    } else {
        let cos_theta_t = (1.0 - sin_theta_i * sin_theta_i).max(0.0).sqrt();
        let r_parl = ((eta_t * cos_theta_i) - (eta_i * cos_theta_t))
            / ((eta_t * cos_theta_i) + (eta_i * cos_theta_t));
        let r_perp = ((eta_i * cos_theta_i) - (eta_t * cos_theta_t))
            / ((eta_i * cos_theta_i) + (eta_t * cos_theta_t));

        (r_parl * r_parl + r_perp * r_perp) / 2.0
    }
}

fn refract(
    in_direction: Vector3<f32>,
    normal: Vector3<f32>,
    relative_ior: f32,
) -> Option<Vector3<f32>> {
    let cos_theta_i = normal.dot(in_direction);
    let sin_2_theta_i = (1.0 - cos_theta_i * cos_theta_i).max(0.0);
    let sin_2_theta_t = relative_ior * relative_ior * sin_2_theta_i;

    if sin_2_theta_t >= 1.0 {
        None // Total internal reflection
    } else {
        let cos_theta_t = (1.0 - sin_2_theta_t).sqrt();
        Some(relative_ior * -in_direction + (relative_ior * cos_theta_i - cos_theta_t) * normal)
    }
}

#[derive(Copy, Clone)]
pub(crate) struct Properties {
    pub(crate) ior: f32,
    pub(crate) env_ior: f32,
    pub(crate) dispersion: f32,
    pub(crate) env_dispersion: f32,
}
