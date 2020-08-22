use cgmath::Vector3;

pub const DIST_EPSILON: f32 = 0.0001;

pub mod utils {
    use std::{
        self,
        ops::{Add, Mul, Sub},
    };

    use rand::Rng;

    use super::DIST_EPSILON;
    use cgmath::{InnerSpace, Vector3};

    pub struct Interpolated<P = Vec<(f32, f32)>> {
        pub points: P,
    }

    impl<P> Interpolated<P> {
        pub fn get<T>(&self, input: f32) -> T
        where
            P: AsRef<[(f32, T)]>,
            T: Clone + Default + Add<Output = T> + Sub<Output = T> + Mul<f32, Output = T>,
        {
            let points = self.points.as_ref();
            if points.len() == 0 {
                return Default::default();
            }

            let mut min = 0;
            let mut max = points.len() - 1;

            let (min_x, _min_y) = points[min].clone();

            if min_x >= input {
                return Default::default(); // min_y
            }

            let (max_x, _max_y) = points[max].clone();

            if max_x <= input {
                return Default::default(); // max_y
            }

            while max > min + 1 {
                let check = (max + min) / 2;
                let (check_x, check_y) = points[check].clone();

                if check_x == input {
                    return check_y;
                }

                if check_x > input {
                    max = check
                } else {
                    min = check
                }
            }

            let (min_x, min_y) = points[min].clone();
            let (max_x, max_y) = points[max].clone();

            if input < min_x {
                Default::default() //min_y
            } else if input > max_x {
                Default::default() //max_y
            } else {
                min_y.clone() + (max_y - min_y) * ((input - min_x) / (max_x - min_x))
            }
        }
    }

    pub fn schlick(
        ref_index1: f32,
        ref_index2: f32,
        normal: Vector3<f32>,
        incident: Vector3<f32>,
    ) -> f32 {
        let mut cos_psi = -normal.dot(incident);
        let r0 = (ref_index1 - ref_index2) / (ref_index1 + ref_index2);

        if ref_index1 > ref_index2 {
            let n = ref_index1 / ref_index2;
            let sin_t2 = n * n * (1.0 - cos_psi * cos_psi);
            if sin_t2 > 1.0 {
                return 1.0;
            }
            cos_psi = (1.0 - sin_t2).sqrt();
        }

        let inv_cos = 1.0 - cos_psi;

        return r0 * r0 + (1.0 - r0 * r0) * inv_cos * inv_cos * inv_cos * inv_cos * inv_cos;
    }

    pub fn ortho(v: Vector3<f32>) -> Vector3<f32> {
        let unit = if v.x.abs() < DIST_EPSILON {
            Vector3::unit_x()
        } else if v.y.abs() < DIST_EPSILON {
            Vector3::unit_y()
        } else if v.z.abs() < DIST_EPSILON {
            Vector3::unit_z()
        } else {
            Vector3 {
                x: -v.y,
                y: v.x,
                z: 0.0,
            }
        };

        v.cross(unit)
    }

    /// Creates two orthogonal vectors that forms the basis of a vector space
    /// together with the input vector. If the input vector is `x`, the output
    /// vectors are `(y, z)`.
    pub fn basis(x: Vector3<f32>) -> (Vector3<f32>, Vector3<f32>) {
        let z = ortho(x).normalize();
        let y = z.cross(x).normalize();
        (y, z)
    }

    pub fn sample_cone<R: ?Sized + Rng>(
        rng: &mut R,
        direction: Vector3<f32>,
        cos_half: f32,
    ) -> Vector3<f32> {
        let o1 = ortho(direction).normalize();
        let o2 = direction.cross(o1).normalize();
        let r1: f32 = std::f32::consts::PI * 2.0 * rng.gen::<f32>();
        let r2: f32 = cos_half + (1.0 - cos_half) * rng.gen::<f32>();
        let oneminus = (1.0 - r2 * r2).sqrt();

        o1 * r1.cos() * oneminus + o2 * r1.sin() * oneminus + &direction * r2
    }

    pub fn solid_angle(cos_half: f32) -> f32 {
        if cos_half >= 1.0 {
            0.0
        } else {
            2.0 * std::f32::consts::PI * (1.0 - cos_half)
        }
    }

    pub fn sample_sphere<R: ?Sized + Rng>(rng: &mut R) -> Vector3<f32> {
        let u = rng.gen::<f32>();
        let v = rng.gen::<f32>();
        let theta = 2.0 * std::f32::consts::PI * u;
        let phi = (2.0 * v - 1.0).acos();
        Vector3::new(phi.sin() * theta.cos(), phi.sin() * theta.sin(), phi.cos())
    }

    pub fn sample_hemisphere<R: ?Sized + Rng>(
        rng: &mut R,
        direction: Vector3<f32>,
    ) -> Vector3<f32> {
        let s = sample_sphere(rng);
        let x = ortho(direction).normalize_to(s.x);
        let y = x.cross(direction).normalize_to(s.y);
        let z = direction.normalize_to(s.z.abs());
        x + y + z
    }
}

pub(crate) fn fresnel(ior: f32, env_ior: f32, normal: Vector3<f32>, incident: Vector3<f32>) -> f32 {
    use cgmath::InnerSpace;

    if incident.dot(normal) < 0.0 {
        utils::schlick(env_ior, ior, normal, incident)
    } else {
        utils::schlick(ior, env_ior, -normal, incident)
    }
}

pub(crate) fn blackbody(wavelength: f32, temperature: f32) -> f32 {
    let wavelength = wavelength * 1.0e-9;
    let power_term = 3.74183e-16 * wavelength.powi(-5);

    power_term / ((1.4388e-2 / (wavelength * temperature)).exp() - 1.0)
}
