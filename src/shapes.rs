use extra::json;
use nalgebra::na;
use nalgebra::na::Vec3;
use core::{SceneObject, Ray};

pub fn from_json(config: &~json::Object, material_count: uint) -> Option<~SceneObject: Send+Freeze> {
	match config.find(&~"type") {

		Some(&json::String(~"Sphere")) => {
			Some(Sphere::from_json(config, material_count))
		},

		Some(&json::String(ref something)) => {
			println!("Error: Unknown object type \"{}\".", something.to_owned());
			None
		},

		_ => None
	}
}



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
				if witch_plane != i {
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
	material: uint
}

impl Sphere {
	pub fn new(position: Vec3<f32>, radius: f32, material: uint) -> Sphere {
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

	pub fn from_json(config: &~json::Object, material_count: uint) -> ~SceneObject: Send+Freeze {
		let label = match config.find(&~"label") {
			Some(&json::String(ref label)) => label.to_owned(),
			_ => ~"<Sphere>"
		};

		let position = match config.find(&~"position") {
			Some(&json::List(ref position)) => {
				if position.len() == 3 {
					let mut new_position: Vec3<f32> = na::zero();
					match position[0] {
						json::Number(x) => {
							new_position.x = x as f32;
						},
						_ => println!("Warning: \"position\" for object \"{}\" must be a list of 3 numbers. Default will be used.", label)
					}

					match position[1] {
						json::Number(y) => {
							new_position.y = y as f32;
						},
						_ => println!("Warning: \"position\" for object \"{}\" must be a list of 3 numbers. Default will be used.", label)
					}

					match position[2] {
						json::Number(z) => {
							new_position.z = z as f32;
						},
						_ => println!("Warning: \"position\" for object \"{}\" must be a list of 3 numbers. Default will be used.", label)
					}

					new_position
				} else {
					println!("Warning: \"position\" for object \"{}\" must be a list of 3 numbers. Default will be used.", label);
					na::zero()
				}
			},
			_ => {
				na::zero()
			}
		};

		let radius = match config.find(&~"radius") {
			Some(&json::Number(radius)) => radius as f32,
			None => {
				1.0
			},
			_ => {
				println!("Warning: \"radius\" for object \"{}\" must be a number. Default will be used.", label);
				1.0
			}
		};

		let material = match config.find(&~"material") {
			Some(&json::Number(i)) => {
				let index = i as uint;
				if index < material_count {
					index
				} else {
					println!("Warning: Unknown material for object \"{}\". Default will be used.", label);
					material_count
				}
			},
			_ => {
				println!("Warning: \"material\" for object \"{}\" is not set. Default will be used.", label);
				material_count
			}
		};

		~Sphere::new(position, radius, material) as ~SceneObject: Send+Freeze
	}
}

impl SceneObject for Sphere {
	fn get_material_index(&self, normal: Ray, ray_in: Ray) -> uint {
		self.material
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