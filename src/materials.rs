extern mod std;
use std::vec;
use nalgebra::na;
use nalgebra::na::Vec3;
use core::{Ray, Material, RandomVariable, Reflection};

//Diffuse
struct Diffuse {
	color: f32
}

impl Material for Diffuse {
	fn get_reflection(&self, normal: Ray, _: Ray, rand_var: &mut RandomVariable) -> Reflection {
		let u = rand_var.next();
		let v = rand_var.next();
		let theta = 2.0 * std::f32::consts::PI * u;
		let phi = std::num::acos(2.0 * v - 1.0);
		let sphere_point = Vec3::new(
			phi.sin() * theta.cos(),
			phi.sin() * theta.sin(),
			phi.cos().abs()
			);

		let mut bases = vec::with_capacity(3);

		na::orthonormal_subspace_basis(&normal.direction, |base| {
			bases.push(base);
			true
		});
		bases.push(normal.direction);

		let mut reflection: Vec3<f32> = na::zero();

		unsafe {
			for (i, base) in bases.iter().enumerate() {
				reflection = reflection + base * sphere_point.at_fast(i);
			}
		}

		Reflection {
			out: Ray::new(normal.origin, reflection),
			color: self.color,
			emission: 0.0
		}
	}
}


//Mirror
struct Mirror {
	color: f32
}

impl Material for Mirror {
	fn get_reflection(&self, normal: Ray, ray_in: Ray, _: &mut RandomVariable) -> Reflection {
		let perp = na::dot(&ray_in.direction, &normal.direction) * 2.0;
		Reflection {
			out: Ray::new(normal.origin, ray_in.direction - (normal.direction * perp)),
			color: self.color,
			emission: 0.0
		}
	}
}


//Emission
struct Emission {
    color: f32,
    luminance: f32
}

impl Material for Emission {
	fn get_reflection(&self, _: Ray, _: Ray, _: &mut RandomVariable) -> Reflection {
		Reflection {
			out: Ray::new(na::zero(), na::zero()),
			color: 0.0,
			emission: self.color * self.luminance
		}
	}
}