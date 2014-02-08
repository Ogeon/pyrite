use extra::json;
use nalgebra::na;
use nalgebra::na::Vec3;
use core::{SceneObject, Ray};
use std::num::{FromPrimitive, Zero, from_f64};

pub fn from_json(config: &~json::Object, material_count: uint) -> Option<~SceneObject: Send+Freeze> {
	match config.find(&~"type") {

		Some(&json::String(~"Sphere")) => {
			Some(Sphere::from_json(config, material_count))
		},

		Some(&json::String(~"Triangle")) => {
			Some(Triangle::from_json(config, material_count))
		},

		Some(&json::String(ref something)) => {
			println!("Error: Unknown object type \"{}\".", something.to_owned());
			None
		},

		_ => None
	}
}


fn parse_vector<T: FromPrimitive+Zero>(config:&~json::Object, key: ~str, label: &str) -> Vec3<T> {
	match config.find(&key) {
		Some(&json::List(ref values)) => {
			if values.len() == 3 {
				let mut vector: Vec3<T> = na::zero();

				match values[0] {
					json::Number(x) => {
						vector.x = from_f64(x).unwrap();
					},
					_ => println!("Warning: \"{}\" for object \"{}\" must be a list of 3 numbers. Default will be used.", key, label)
				}

				match values[1] {
					json::Number(y) => {
						vector.y = from_f64(y).unwrap();
					},
					_ => println!("Warning: \"{}\" for object \"{}\" must be a list of 3 numbers. Default will be used.", key, label)
				}

				match values[2] {
					json::Number(z) => {
						vector.z = from_f64(z).unwrap();
					},
					_ => println!("Warning: \"{}\" for object \"{}\" must be a list of 3 numbers. Default will be used.", key, label)
				}

				vector
			} else {
				println!("Warning: \"{}\" for object \"{}\" must be a list of 3 numbers. Default will be used.", key, label);
				na::zero()
			}
		},
		_ => {
			na::zero()
		}
	}
}

fn parse_material_inidex(config:&~json::Object, max_index: uint, label: &str) -> uint {
	match config.find(&~"material") {
		Some(&json::Number(i)) => {
			let index = i as uint;
			if index < max_index {
				index
			} else {
				println!("Warning: Unknown material for object \"{}\". Default will be used.", label);
				max_index
			}
		},
		_ => {
			println!("Warning: \"material\" for object \"{}\" is not set. Default will be used.", label);
			max_index
		}
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
	material: uint
}

impl Sphere {
	pub fn new(position: Vec3<f32>, radius: f32, material: uint) -> Sphere {
		Sphere {
			position: position,
			radius: radius,
			material: material
		}
	}

	pub fn from_json(config: &~json::Object, material_count: uint) -> ~SceneObject: Send+Freeze {
		let label = match config.find(&~"label") {
			Some(&json::String(ref label)) => label.to_owned(),
			_ => ~"<Sphere>"
		};

		let position = parse_vector(config, ~"position", label);

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

		let material = parse_material_inidex(config, material_count, label);

		~Sphere::new(position, radius, material) as ~SceneObject: Send+Freeze
	}
}

impl SceneObject for Sphere {
	fn get_material_index(&self, _: Ray, _: Ray) -> uint {
		self.material
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


//Triangle
struct Triangle {
	v1: Vec3<f32>,
	v2: Vec3<f32>,
	v3: Vec3<f32>,
	material: uint
}

impl Triangle {
	fn from_json(config: &~json::Object, material_count: uint) -> ~SceneObject: Send+Freeze {
		let label = match config.find(&~"label") {
			Some(&json::String(ref label)) => label.to_owned(),
			_ => ~"<Triangle>"
		};

		let v1 = parse_vector(config, ~"v1", label);
		let v2 = parse_vector(config, ~"v2", label);
		let v3 = parse_vector(config, ~"v3", label);

		let material = parse_material_inidex(config, material_count, label);

		~Triangle{v1: v1, v2: v2, v3: v3, material: material} as ~SceneObject: Send+Freeze
	}
}

impl SceneObject for Triangle {
	fn get_material_index(&self, _: Ray, _: Ray) -> uint {
		self.material
	}

	//Möller–Trumbore intersection algorithm
	fn intersect(&self, ray: Ray) -> Option<(Ray, f32)> {
		let epsilon = 0.000001f32;
		let e1 = self.v2 - self.v1;
		let e2 = self.v3 - self.v1;

		let p = na::cross(&ray.direction, &e2);
		let det = na::dot(&e1, &p);

		if det > -epsilon && det < epsilon {
			return None;
		}

		let inv_det = 1.0 / det;
		let t = ray.origin - self.v1;
		let u = na::dot(&t, &p) * inv_det;

		//Outside triangle
		if u < 0.0 || u > 1.0 {
			return None;
		}

		let q = na::cross(&t, &e1);
		let v = na::dot(&ray.direction, &q) * inv_det;

		//Outside triangle
		if(v < 0.0 || u + v > 1.0) {
			return None;
		}

		let dist = na::dot(&e2, &q) * inv_det;
		if dist > epsilon {
			let hit_position = ray.origin + (ray.direction * dist);
			Some((Ray::new(hit_position, na::cross(&e1, &e2)), dist))
		} else {
			None
		}
	}
}