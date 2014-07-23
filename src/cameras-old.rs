use std::f64::consts::PI;
use std::f32;
use std::num::sqrt;
use extra::json;
use nalgebra::na;
use nalgebra::na::{Vec3, Rot3, Rotate};
use core::{Ray, Camera, RandomVariable};

pub fn from_json(config: &~json::Object) -> Option<~Camera: Send+Freeze> {
	match config.find(&~"type") {

		Some(&json::String(~"Perspective")) => {
			Some(Perspective::from_json(config))
		}

		_ => None
	}
}


pub struct Perspective {
	position: Vec3<f32>,
	rotation: Rot3<f32>,
	lens: f32,
	aperture: f32,
	focal_distance: f32
}

impl Perspective {
	pub fn look_at(from: Vec3<f32>, to: Vec3<f32>, up: Vec3<f32>) -> Perspective {
		let mut rot = Rot3::new(na::zero());
		rot.look_at_z(&(to - from), &up);
		Perspective{
			position: from,
			rotation: rot,
			lens: 2.0,
			aperture: 0.0,
			focal_distance: 0.0
		}
	}

	pub fn from_json(config: &~json::Object) -> ~Camera: Send+Freeze {
		let position = match config.find(&~"position") {
			Some(&json::List(ref position)) => {
				let mut vector: Vec3<f32> = na::zero();

				if position.len() == 3 {
					match position[0] {
						json::Number(x) => {
							vector.x = x as f32;
						},
						_ => println!("Warning: Camera position must be a list of 3 numbers. Default will be used.")
					}

					match position[1] {
						json::Number(y) => {
							vector.y = y as f32;
						},
						_ => println!("Warning: Camera position must be a list of 3 numbers. Default will be used.")
					}

					match position[2] {
						json::Number(z) => {
							vector.z = z as f32;
						},
						_ => println!("Warning: Camera position must be a list of 3 numbers. Default will be used.")
					}
				} else {
					println!("Warning: Camera position must be a list of 3 numbers. Default will be used.");
				}

				vector
			},
			_ => na::zero()
		};

		let rotation = match config.find(&~"rotation") {
			Some(&json::List(ref rotation)) => {
				let mut new_rotation: Vec3<f32> = na::zero();

				if rotation.len() == 3 {
					match rotation[0] {
						json::Number(x) => {
							new_rotation.x = (x * PI / 180.0) as f32;
						},
						_ => println!("Warning: Camera rotation must be a list of 3 numbers. Default will be used.")
					}

					match rotation[1] {
						json::Number(y) => {
							new_rotation.y = (y * PI / 180.0) as f32;
						},
						_ => println!("Warning: Camera rotation must be a list of 3 numbers. Default will be used.")
					}

					match rotation[2] {
						json::Number(z) => {
							new_rotation.z = (z * PI / 180.0) as f32;
						},
						_ => println!("Warning: Camera rotation must be a list of 3 numbers. Default will be used.")
					}
				} else {
					println!("Warning: Camera rotation must be a list of 3 numbers. Default will be used.");
				}

				Rot3::new(new_rotation)
			},
			_ => Rot3::new(na::zero())
		};

		let lens = match config.find(&~"lens") {
			Some(&json::Number(lens)) => lens as f32,
			_ => 2.0
		};

		let aperture = match config.find(&~"aperture") {
			Some(&json::Number(aperture)) => aperture as f32,
			_ => 0.0
		};

		let focal_distance = match config.find(&~"focal_distance") {
			Some(&json::Number(focal_distance)) => focal_distance as f32,
			_ => 0.0
		};

		~Perspective{
			position: position,
			rotation: rotation,
			lens: lens,
			aperture: aperture,
			focal_distance: focal_distance
		} as ~Camera: Send+Freeze
	}
}

impl Camera for Perspective {
	fn ray_to(&self, x: f32, y: f32, rand_var: &mut RandomVariable) -> Ray {
		if self.aperture == 0.0 {
			Ray::new(self.position, self.rotation.rotate(&Vec3::new(x, -y, -self.lens)))
		} else {
			let base_dir = Vec3::new(x / self.lens, -y / self.lens, -1.0);
			let focal_point = base_dir * self.focal_distance;

			let sqrt_r = sqrt(rand_var.next() * self.aperture);
			let psi = rand_var.next() * 2.0 * f32::consts::PI;
			let lens_x = sqrt_r * psi.cos();
			let lens_y = sqrt_r * psi.sin();

			let lens_point = Vec3::new(lens_x, lens_y, 0.0);
			
			Ray::new(self.rotation.rotate(&lens_point) + self.position, self.rotation.rotate(&(focal_point - lens_point)))
		}
	}
}