use nalgebra::na;
use nalgebra::na::Vec3;
use core::{SceneObject, Ray, Material, RandomVariable, Reflection};

//Bounding Box
priv struct BoundingBox {
	from: Vec3<f32>,
	to: Vec3<f32>
}

impl BoundingBox {
	fn intersect(&self, ray: Ray) -> Option<f32> {
		let origin = ray.origin;
		let dir = ray.direction;

		let mut quadrant = [Left, Left, Left];
		let mut candidate_plane = [0f32, ..3];
		let mut inside = true;

		let mut coord: Vec3<f32> = na::zero();
		let mut max_t = [0f32, ..3];
		let mut witch_plane = 0;

		unsafe {
			for i in range(0 as uint, 3) {
				if origin.at_fast(i) < self.from.at_fast(i) {
					candidate_plane[i] = self.from.at_fast(i);
					inside = false;
				} else if origin.at_fast(i) > self.to.at_fast(i) {
					quadrant[i] = Right;
					candidate_plane[i] = self.to.at_fast(i);
					inside = false;
				} else {
					quadrant[i] = Middle;
				}
			}
		}

		if inside {
			return Some(0.0);
		}

		unsafe {
			for i in range(0 as uint, 3) {
				if quadrant[i] != Middle && dir.at_fast(i) != 0.0 {
					max_t[i] = (candidate_plane[i] - origin.at_fast(i)) / dir.at_fast(i);
				} else {
					max_t[i] = -1.0;
				}
			}
		}

		for (i, &v) in max_t.iter().enumerate() {
			if v > max_t[witch_plane] {
				witch_plane = i;
			}
		}

		if max_t[witch_plane] < 0.0 {
			return None;
		}

		unsafe {
			for i in range(0 as uint, 3) {
				if(witch_plane != i) {
					coord.set_fast(i, origin.at_fast(i) + max_t[witch_plane] * dir.at_fast(i));
					if coord.at_fast(i) < self.from.at_fast(i) || coord.at_fast(i) > self.to.at_fast(i) {
						return None;
					}
				} else {
					coord.set_fast(i, candidate_plane[i]);
				}
			}
		}

		return Some(na::norm(&(coord - origin)));
	}
}

enum Quadrant {
	Left = 0,
	Middle = 1,
	Right = 2
}

impl Eq for Quadrant {
	fn eq(&self, other: &Quadrant) -> bool {
		*self as int == *other as int
	}

	fn ne(&self, other: &Quadrant) -> bool {
		*self as int != *other as int
	}
}


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
	fn get_reflection(&self, normal: Ray, ray_in: Ray, frequency: f32, rand_var: &mut RandomVariable) -> Reflection {
		self.material.get_reflection(normal, ray_in, frequency, rand_var)
	}

	fn get_proximity(&self, ray: Ray) -> Option<f32> {
		self.bounds.intersect(ray)
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