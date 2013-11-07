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
	fn get_reflection(&self, normal: Ray, _: Ray, _: f32, rand_var: &mut RandomVariable) -> Reflection {
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
			emission: 0.0,
			dispersion: false
		}
	}
}


//Mirror
struct Mirror {
	color: f32
}

impl Material for Mirror {
	fn get_reflection(&self, normal: Ray, ray_in: Ray, _: f32, _: &mut RandomVariable) -> Reflection {
		let perp = na::dot(&ray_in.direction, &normal.direction) * 2.0;
		Reflection {
			out: Ray::new(normal.origin, ray_in.direction - (normal.direction * perp)),
			color: self.color,
			emission: 0.0,
			dispersion: false
		}
	}
}


//Emission
struct Emission {
    color: f32,
    luminance: f32
}

impl Material for Emission {
	fn get_reflection(&self, _: Ray, _: Ray, _: f32, _: &mut RandomVariable) -> Reflection {
		Reflection {
			out: Ray::new(na::zero(), na::zero()),
			color: 0.0,
			emission: self.color * self.luminance,
			dispersion: false
		}
	}
}


//Mix
struct Mix {
	material_a: ~Material: Send + Freeze,
	material_b: ~Material: Send + Freeze,
	factor: f32
}


impl Material for Mix {
	fn get_reflection(&self, normal: Ray, ray_in: Ray, frequency: f32, rand_var: &mut RandomVariable) -> Reflection {
		if rand_var.next() > self.factor {
			self.material_a.get_reflection(normal, ray_in, frequency, rand_var)
		} else {
			self.material_b.get_reflection(normal, ray_in, frequency, rand_var)
		}
	}
}


//Fresnel mix
struct FresnelMix {
	reflection: ~Material: Send + Freeze,
	refraction: ~Material: Send + Freeze,
	refractive_index: f32,
	dispersion: f32
}

impl Material for FresnelMix {
	fn get_reflection(&self, normal: Ray, ray_in: Ray, frequency: f32, rand_var: &mut RandomVariable) -> Reflection {
		let ref_index = self.refractive_index + self.dispersion/frequency;

		let factor = if na::dot(&ray_in.direction, &normal.direction) < 0.0 {
			FresnelMix::schlick(1.0, ref_index, normal.direction, ray_in.direction)
		} else {
			FresnelMix::schlick(ref_index, 1.0, -normal.direction, ray_in.direction)
		};

		let mut reflection = if rand_var.next() < factor {
			self.reflection.get_reflection(normal, ray_in, frequency, rand_var)
		} else {
			self.refraction.get_reflection(normal, ray_in, frequency, rand_var)
		};

		reflection.dispersion = reflection.dispersion || self.dispersion != 0.0;
		return reflection;
	}
}

impl FresnelMix {
	fn schlick(ref_index1: f32, ref_index2: f32, normal: Vec3<f32>, incident: Vec3<f32>) -> f32 {
		let mut cos_psi = -na::dot(&normal, &incident);
		let r0 = (ref_index1 - ref_index2) / (ref_index1 + ref_index2);

		if ref_index1 > ref_index2 {
			let n = ref_index1 / ref_index2;
			let sinT2 = n * n * (1.0 - cos_psi * cos_psi);
			if sinT2 > 1.0 {
				return 1.0;
			}
			cos_psi = (1.0 - sinT2).sqrt();
		}

		let inv_cos = 1.0 - cos_psi;

		return r0 * r0 + (1.0 - r0 * r0) * inv_cos * inv_cos * inv_cos * inv_cos * inv_cos;
	}
}


//Refractive
struct Refractive {
	color: f32,
	refractive_index: f32,
	dispersion: f32
}

impl Material for Refractive {
	fn get_reflection(&self, normal: Ray, ray_in: Ray, frequency: f32, _: &mut RandomVariable) -> Reflection {
		let dot = na::dot(&ray_in.direction, &normal.direction);
		let eta = if dot < 0.0 {
			1.0/(self.refractive_index + self.dispersion/frequency)
		} else {
			(self.refractive_index + self.dispersion/frequency)
		};

		let norm = if dot < 0.0 {
			normal.direction
		} else {
			-normal.direction
		};

		let c1 = -na::dot(&ray_in.direction, &norm);

		let cs2 = 1.0 - eta*eta*(1.0 - c1*c1);
		if cs2 < 0.0 {
			return Reflection {
				out: Ray::new(na::zero(), na::zero()),
				color: 0.0,
				emission: 0.0,
				dispersion: false
			}
		}

		return Reflection {
			out: Ray::new(normal.origin, ray_in.direction*eta + norm*(eta*c1 - cs2.sqrt())),
			color: self.color,
			emission: 0.0,
			dispersion: self.dispersion != 0.0
		}
	}
}