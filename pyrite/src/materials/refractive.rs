use cgmath::{InnerSpace, Vector3};

use super::Scattering;
use rand::Rng;

pub(crate) fn scatter(
    properties: &Properties,
    in_direction: Vector3<f32>,
    normal: Vector3<f32>,
    wavelength: f32,
    rng: &mut impl Rng,
) -> Scattering {
    let dispersed = properties.dispersion != 0.0 || properties.env_dispersion != 0.0;

    let (out_direction, probability) = if dispersed {
        let wl = wavelength * 0.001;
        let ior = properties.ior + properties.dispersion / (wl * wl);
        let env_ior = properties.env_ior + properties.env_dispersion / (wl * wl);
        refract(ior, env_ior, in_direction, normal, rng)
    } else {
        refract(
            properties.ior,
            properties.env_ior,
            in_direction,
            normal,
            rng,
        )
    };

    Scattering::Reflected {
        out_direction,
        probability,
        dispersed,
        brdf: None,
    }
}

#[derive(Copy, Clone)]
pub(crate) struct Properties {
    pub(crate) ior: f32,
    pub(crate) env_ior: f32,
    pub(crate) dispersion: f32,
    pub(crate) env_dispersion: f32,
}

fn refract<'a, R: Rng>(
    ior: f32,
    env_ior: f32,
    in_direction: Vector3<f32>,
    normal: Vector3<f32>,
    rng: &mut R,
) -> (Vector3<f32>, f32) {
    let nl = if normal.dot(in_direction) < 0.0 {
        normal
    } else {
        -normal
    };

    let reflected = in_direction - (normal * 2.0 * normal.dot(in_direction));

    let into = normal.dot(nl) > 0.0;

    let nnt = if into { env_ior / ior } else { ior / env_ior };
    let ddn = in_direction.dot(nl);

    let cos2t = 1.0 - nnt * nnt * (1.0 - ddn * ddn);
    if cos2t < 0.0 {
        // Total internal reflection
        return (reflected, 1.0);
    }

    let s = if into { 1.0 } else { -1.0 } * (ddn * nnt + cos2t.sqrt());
    let tdir = (in_direction * nnt - normal * s).normalize();

    let a = ior - env_ior;
    let b = ior + env_ior;
    let r0 = a * a / (b * b);
    let c = 1.0 - if into { -ddn } else { tdir.dot(normal) };

    let re = r0 + (1.0 - r0) * c * c * c * c * c;
    let tr = 1.0 - re;
    let p = 0.25 + 0.5 * re;
    let rp = re / p;
    let tp = tr / (1.0 - p);

    if rng.gen::<f32>() < p {
        (reflected, rp)
    } else {
        (tdir, tp)
    }
}
