use nalgebra::na;
use nalgebra::na::Vec3;
use core::{BoundingBox, SceneObject, Ray, Material, RandomVariable, Reflection};
//Sphere
struct Sphere {
	position: Vec3<f32>,
	radius: f32,
	bounds: BoundingBox,
	material: ~Material: Send + Freeze
}

impl Sphere {
	pub fn new(position: Vec3<f32>, radius: f32, material: ~Material: Send+Freeze) -> Sphere {
		Sphere {
			position: position,
			radius: radius,
			bounds: BoundingBox {
				from: Vec3::new(-radius, -radius, -radius) + position,
				to: Vec3::new(radius, radius, radius) + position
			},
			material: material
		}
	}
}

impl SceneObject for Sphere {
	fn get_reflection(&self, normal: Ray, ray_in: Ray, rand_var: &mut RandomVariable) -> Reflection {
		self.material.get_reflection(normal, ray_in, rand_var)
	}

	fn get_bounds(&self) -> BoundingBox {
		self.bounds
	}

	fn intersect(&self, ray: Ray) -> Option<(Ray, f32)> {
		let diff = ray.origin - self.position;
		let a0 = na::dot(&diff, &diff) - self.radius*self.radius;

		if a0 <= 0.0 {
			let a1 = na::dot(&ray.direction, &diff);
			let discr = a1*a1 - a0;
			let root = discr.sqrt();
			let dist = root - a1;
			let hit_position = ray.origin + (ray.direction * dist);
			return Some((Ray::new(hit_position, hit_position - self.position), dist));
		}

		let a1 = na::dot(&ray.direction, &diff);
		if a1 >= 0.0 {
			return None;
		}

		let discr = a1*a1 - a0;
		if discr < 0.0 {
			return None
		} else if discr >= 0.0 {
			let root = discr.sqrt();
			let dist = -a1 - root;
			let hit_position = ray.origin + (ray.direction * dist);
			return Some((Ray::new(hit_position, hit_position - self.position), dist));
		} else {
			let dist = -a1;
			let hit_position = ray.origin + (ray.direction * dist);
			return Some((Ray::new(hit_position, hit_position - self.position), dist));
		}
	}
}