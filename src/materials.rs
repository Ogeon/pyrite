use std;

use cgmath::vector::{EuclideanVector, Vector, Vector3};
use cgmath::ray::{Ray, Ray3};

use tracer::{Material, FloatRng, Reflection, ParametricValue, Emit, Reflect};

pub struct Diffuse<V> {
    pub reflection: V
}

impl<V: ParametricValue<f64, f64> + 'static> Material for Diffuse<V> {
    fn reflect(&self, ray_in: &Ray3<f64>, normal: &Ray3<f64>, rng: &mut FloatRng) -> Reflection {
        let u = rng.next_float();
        let v = rng.next_float();
        let theta = 2.0f64 * std::f64::consts::PI * u;
        let phi = (2.0 * v - 1.0).acos();
        let sphere_point = Vector3::new(
            phi.sin() * theta.cos(),
            phi.sin() * theta.sin(),
            phi.cos().abs()
            );

        let mut n = if ray_in.direction.dot(&normal.direction) < 0.0 {
            normal.direction
        } else {
            -normal.direction
        };

        let mut reflected = n.cross(&if n.x > 0.3 {
            Vector3::new(n.x, 0.0, 0.0)
        } else if n.y > 0.3 {
            Vector3::new(0.0, n.y, 0.0)
        } else {
            Vector3::new(0.0, 0.0, n.z)
        });

        reflected.normalize_self_to(sphere_point.x);

        let mut y = n.cross(&reflected);
        y.normalize_self_to(sphere_point.y);

        reflected.add_self_v(&y);

        n.normalize_self_to(sphere_point.z);
        reflected.add_self_v(&n);

        Reflect(Ray::new(normal.origin, reflected), &self.reflection as &ParametricValue<f64, f64>)
    }
}

pub struct Emission<V> {
    pub spectrum: V
}

impl<V: ParametricValue<f64, f64> + 'static> Material for Emission<V> {
    fn reflect(&self, _ray_in: &Ray3<f64>, _normal: &Ray3<f64>, _rng: &mut FloatRng) -> Reflection {
        Emit(&self.spectrum as &ParametricValue<f64, f64>)
    }
}