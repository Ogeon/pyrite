use cgmath::{InnerSpace, Vector3};

use rand::Rng;

use super::Scattering;
use crate::math::utils::sample_hemisphere;

pub(crate) fn scatter(
    in_direction: Vector3<f32>,
    normal: Vector3<f32>,
    rng: &mut impl Rng,
) -> Scattering {
    let normal = if in_direction.dot(normal) < 0.0 {
        normal
    } else {
        -normal
    };

    Scattering::Reflected {
        out_direction: sample_hemisphere(rng, normal),
        probability: 1.0,
        dispersed: false,
        brdf: Some(lambertian),
    }
}

fn lambertian(_ray_in: Vector3<f32>, ray_out: Vector3<f32>, normal: Vector3<f32>) -> f32 {
    2.0 * normal.dot(ray_out).abs()
}
