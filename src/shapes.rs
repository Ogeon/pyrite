use cgmath::sphere;
use cgmath::transform::{Transform, Decomposed};
use cgmath::vector::{EuclideanVector, Vector3};
use cgmath::point::{Point, Point3};
use cgmath::intersect::Intersect;
use cgmath::ray::{Ray, Ray3};
use cgmath::quaternion::Quaternion;

pub enum Shape {
	Sphere { pub position: Point3<f64>, pub radius: f64 }
}

impl Shape {
	pub fn intersect(&self, ray: &Ray3<f64>) -> Option<Ray3<f64>> {
		match *self {
			Sphere {ref position, radius} => {
				let sphere = sphere::Sphere {
					radius: radius,
					center: position.clone()
				};
				(sphere, ray.clone()).intersection()
					.map(|intersection| Ray::new(intersection, intersection.to_vec().normalize()))
			}
		}
	}
}