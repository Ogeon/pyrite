use cgmath::{InnerSpace, Point2, Vector3};

use crate::{
    light::Wavelengths,
    math::face_forward,
    tracer::{LightProgram, RenderContext},
    utils::Tools,
};

use super::SurfaceInteraction;

// Mostly based on the PBR book.
pub(super) fn sample_reflection<'t, 'a>(
    properties: &Properties,
    out_direction: Vector3<f32>,
    texture_coordinate: Point2<f32>,
    color: LightProgram<'a>,
    wavelengths: &Wavelengths,
    tools: &mut Tools<'t, 'a>,
) -> SurfaceInteraction<'t> {
    let dispersed = properties.dispersion != 0.0 || properties.env_dispersion != 0.0;

    let mut reflectivity = tools.light_pool.get();

    let (ior, env_ior) = if dispersed {
        reflectivity.set_single_wavelength();

        let wl = wavelengths.hero() * 0.001;
        let ior = properties.ior + properties.dispersion / (wl * wl);
        let env_ior = properties.env_ior + properties.env_dispersion / (wl * wl);
        (ior, env_ior)
    } else {
        (properties.ior, properties.env_ior)
    };

    let fresnel = fresnel_dielectric(out_direction.z, env_ior, ior);

    let initial_input = RenderContext {
        wavelength: wavelengths.hero(),
        normal: Vector3::unit_z(),
        ray_direction: -out_direction,
        texture: texture_coordinate,
    };
    let mut color_program = color.memoize(initial_input, tools.execution_context);

    let (in_direction, pdf) = if tools.sampler.gen_f32() < fresnel {
        let in_direction = Vector3::new(-out_direction.x, -out_direction.y, out_direction.z);

        let abs_cos_in = in_direction.z.abs();

        if abs_cos_in > 0.0 {
            for (bin, wavelength) in reflectivity.iter_mut().zip(wavelengths) {
                color_program.update_input().set_wavelength(wavelength);
                *bin = fresnel * color_program.run() / abs_cos_in;
            }
        }

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

        let abs_cos_in = in_direction.z.abs();

        if abs_cos_in > 0.0 {
            for (bin, wavelength) in reflectivity.iter_mut().zip(wavelengths) {
                color_program.update_input().set_wavelength(wavelength);
                *bin = (1.0 - fresnel) * color_program.run() / abs_cos_in;
            }
        }

        (in_direction, 1.0 - fresnel)
    };

    SurfaceInteraction {
        reflectivity,
        pdf: if in_direction.z == 0.0 { 0.0 } else { pdf },
        diffuse: false,
        in_direction,
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
