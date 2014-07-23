use cgmath::sphere::Sphere;
use cgmath::transform::{Transform, Decomposed};
use cgmath::vector::Vector3;
use cgmath::point::Point;
use cgmath::intersect::Intersect;
use cgmath::ray::{Ray, Ray3};
use cgmath::quaternion::Quaternion;

pub enum Shape {
	Sphere(Decomposed<f64, Vector3<f64>, Quaternion<f64>>)
}

impl Shape {
	pub fn intersect(&self, ray: &Ray3<f64>) -> Option<Ray3<f64>> {
		match *self {
			Sphere(ref transform) => {
				transform.invert()
				.and_then(|t| {
					let sphere = Sphere {
						radius: 1.0,
						center: Point::origin()
					};

					let new_ray = t.transform_ray(ray);
					//println!("{} -> {} to {} -> {}", ray.origin, ray.direction, new_ray.origin, new_ray.direction);
					(sphere, new_ray).intersection()
				})
				.map( |intersection| Ray::new(transform.transform_point(&intersection), intersection.to_vec()) )
			}
		}
	}
}