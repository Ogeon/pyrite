use cgmath::{ElementWise, InnerSpace, Vector3};
use collision::{Aabb3, Ray3};

pub const DIST_EPSILON: f32 = 0.0001;

pub mod utils {
    use std::{
        self,
        f32::consts,
        ops::{Add, Mul, Sub},
    };

    use crate::renderer::samplers::Sampler;

    use super::DIST_EPSILON;
    use cgmath::{InnerSpace, Point2, Vector3};

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

    pub(crate) fn sample_cone(
        rng: &mut dyn Sampler,
        direction: Vector3<f32>,
        cos_half: f32,
    ) -> Vector3<f32> {
        let o1 = ortho(direction).normalize();
        let o2 = direction.cross(o1).normalize();
        let r1: f32 = std::f32::consts::PI * 2.0 * rng.gen_f32();
        let r2: f32 = cos_half + (1.0 - cos_half) * rng.gen_f32();
        let oneminus = (1.0 - r2 * r2).sqrt();

        o1 * r1.cos() * oneminus + o2 * r1.sin() * oneminus + &direction * r2
    }

    pub(crate) fn sample_sphere(rng: &mut dyn Sampler) -> Vector3<f32> {
        let u = rng.gen_f32();
        let v = rng.gen_f32();
        let theta = 2.0 * std::f32::consts::PI * u;
        let phi = (2.0 * v - 1.0).acos();
        Vector3::new(phi.sin() * theta.cos(), phi.sin() * theta.sin(), phi.cos())
    }

    /// Samples a unit disk by turning 2D grid squares into concentric circles.
    #[inline(always)]
    pub(crate) fn sample_concentric_disk(sampler: &mut dyn Sampler) -> Point2<f32> {
        let u1 = -1.0 + sampler.gen_f32() * 2.0;
        let u2 = -1.0 + sampler.gen_f32() * 2.0;

        if u1 == 0.0 && u2 == 0.0 {
            Point2::new(0.0, 0.0)
        } else {
            let (theta, radius) = if u1.abs() > u2.abs() {
                (consts::FRAC_PI_4 * (u2 / u1), u1)
            } else {
                (consts::FRAC_PI_2 - consts::FRAC_PI_4 * (u1 / u2), u2)
            };

            radius * Point2::new(theta.cos(), theta.sin())
        }
    }

    /// Samples a cosine weighted unit hemisphere, where positive Z is "up".
    #[inline(always)]
    pub(crate) fn sample_cosine_hemisphere(sampler: &mut dyn Sampler) -> Vector3<f32> {
        let Point2 { x, y } = sample_concentric_disk(sampler);
        let z = 0.0f32.max(1.0 - x * x - y * y).sqrt();
        Vector3::new(x, y, z)
    }
}

pub(crate) fn fresnel(ior: f32, env_ior: f32, normal: Vector3<f32>, incident: Vector3<f32>) -> f32 {
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

pub(crate) fn aabb_intersection_distance(aabb: Aabb3<f32>, ray: Ray3<f32>) -> Option<f32> {
    let inv_dir = Vector3::new(1.0, 1.0, 1.0).div_element_wise(ray.direction);

    let mut t1 = (aabb.min.x - ray.origin.x) * inv_dir.x;
    let mut t2 = (aabb.max.x - ray.origin.x) * inv_dir.x;

    let mut tmin = t1.min(t2);
    let mut tmax = t1.max(t2);

    for i in 1..3 {
        t1 = (aabb.min[i] - ray.origin[i]) * inv_dir[i];
        t2 = (aabb.max[i] - ray.origin[i]) * inv_dir[i];

        tmin = tmin.max(t1.min(t2));
        tmax = tmax.min(t1.max(t2));
    }

    // Add && (tmin >= 0.0 || tmax >= 0.0) back?
    if tmax >= tmin && tmax >= 0.0 {
        Some(tmin.max(0.0))
    } else {
        None
    }
}

/// Checks if two vectors are within the same hemisphere, centered around the Z axis.
#[inline(always)]
pub(crate) fn same_hemisphere(v1: Vector3<f32>, v2: Vector3<f32>) -> bool {
    v1.z * v2.z > 0.0
}

#[inline(always)]
pub(crate) fn power_heuristic(nf: f32, f_pdf: f32, ng: f32, g_pdf: f32) -> f32 {
    let f = nf * f_pdf;
    let g = ng * g_pdf;
    (f * f) / (f * f + g * g)
}

#[inline(always)]
pub(crate) fn face_forward(vector: Vector3<f32>, forward: Vector3<f32>) -> Vector3<f32> {
    if vector.dot(forward) < 0.0 {
        -vector
    } else {
        vector
    }
}
