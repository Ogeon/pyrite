use cgmath::{InnerSpace, Vector3};

use super::Scattering;

pub(crate) fn scatter(in_direction: Vector3<f32>, normal: Vector3<f32>) -> Scattering {
    let mut normal = if in_direction.dot(normal) < 0.0 {
        normal
    } else {
        -normal
    };

    let perp = in_direction.dot(normal) * 2.0;
    normal *= perp;

    Scattering::Reflected {
        out_direction: in_direction - normal,
        probability: 1.0,
        dispersed: false,
        brdf: None,
    }
}
