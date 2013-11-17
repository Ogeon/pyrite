extern mod std;
use std::vec;
use extra::json;
use nalgebra::na;
use nalgebra::na::Vec3;
use core::{Ray, Material, RandomVariable, Reflection};


pub fn from_json(config: &~json::Object) -> Option<~Material: Send+Freeze> {
	match config.find(&~"type") {

		Some(&json::String(~"Diffuse")) => {
			Diffuse::from_json(config)
		},

		Some(&json::String(~"Mirror")) => {
			Mirror::from_json(config)
		},

		Some(&json::String(~"Emission")) => {
			Emission::from_json(config)
		},

		Some(&json::String(~"Refractive")) => {
			Refractive::from_json(config)
		},

		Some(&json::String(~"Mix")) => {
			Mix::from_json(config)
		},

		Some(&json::String(~"FresnelMix")) => {
			FresnelMix::from_json(config)
		},

		Some(&json::String(ref something)) => {
			println!("Warning: Unknown material {}", something.to_owned());
			None
		},

		_ => None
	}
}


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

	fn to_owned_material(&self) -> ~Material: Send+Freeze {
		~Diffuse {
			color: self.color
		} as ~Material: Send+Freeze
	}
}

impl Diffuse {
	pub fn from_json(config: &~json::Object) -> Option<~Material: Send+Freeze> {
		let label = match config.find(&~"label") {
			Some(&json::String(ref label)) => label.to_owned(),
			_ => ~"<Diffuse>"
		};

		let color = match config.find(&~"color") {
			Some(&json::Number(color)) => color as f32,
			_ => {
				println!("Warning: Color for {} is not set", label);
				1.0
			}
		};

		Some(~Diffuse {
			color: color
		} as ~Material: Send+Freeze)
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

	fn to_owned_material(&self) -> ~Material: Send+Freeze {
		~Mirror {
			color: self.color
		} as ~Material: Send+Freeze
	}
}

impl Mirror {
	pub fn from_json(config: &~json::Object) -> Option<~Material: Send+Freeze> {
		let label = match config.find(&~"label") {
			Some(&json::String(ref label)) => label.to_owned(),
			_ => ~"<Mirror>"
		};

		let color = match config.find(&~"color") {
			Some(&json::Number(color)) => color as f32,
			_ => {
				println!("Warning: Color for {} is not set", label);
				1.0
			}
		};

		Some(~Mirror {
			color: color
		} as ~Material: Send+Freeze)
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

	fn to_owned_material(&self) -> ~Material: Send+Freeze {
		~Emission {
			color: self.color,
			luminance: self.luminance
		} as ~Material: Send+Freeze
	}
}

impl Emission {
	pub fn from_json(config: &~json::Object) -> Option<~Material: Send+Freeze> {
		let label = match config.find(&~"label") {
			Some(&json::String(ref label)) => label.to_owned(),
			_ => ~"<Emission>"
		};

		let color = match config.find(&~"color") {
			Some(&json::Number(color)) => color as f32,
			_ => {
				println!("Warning: Color for {} is not set", label);
				1.0
			}
		};

		let luminance = match config.find(&~"luminance") {
			Some(&json::Number(luminance)) => luminance as f32,
			_ => {
				println!("Warning: Luminance for {} is not set", label);
				1.0
			}
		};

		Some(~Emission {
			color: color,
			luminance: luminance
		} as ~Material: Send+Freeze)
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

	fn to_owned_material(&self) -> ~Material: Send+Freeze {
		~Mix {
			material_a: self.material_a.to_owned_material(),
			material_b: self.material_b.to_owned_material(),
			factor: self.factor
		} as ~Material: Send+Freeze
	}
}

impl Mix {
	pub fn from_json(config: &~json::Object) -> Option<~Material: Send+Freeze> {
		let label = match config.find(&~"label") {
			Some(&json::String(ref label)) => label.to_owned(),
			_ => ~"<Mix>"
		};

		let factor = match config.find(&~"factor") {
			Some(&json::Number(factor)) => factor as f32,
			_ => {
				println!("Warning: Factor for {} is not set", label);
				0.5
			}
		};

		let material_a = match config.find(&~"material_a") {
			Some(&json::Object(ref material)) => from_json(material),
			_ => {
				println!("Warning: Invalid material_a for {}", label);
				None
			}
		};

		if material_a.is_none() {
			return None;
		}

		let material_b = match config.find(&~"material_b") {
			Some(&json::Object(ref material)) => from_json(material),
			_ => {
				println!("Warning: Invalid material_b for {}", label);
				None
			}
		};

		if material_b.is_none() {
			return None;
		}

		Some(~Mix {
			material_a: material_a.unwrap(),
			material_b: material_b.unwrap(),
			factor: factor
		} as ~Material: Send+Freeze)
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

	fn to_owned_material(&self) -> ~Material: Send+Freeze {
		~FresnelMix {
			reflection: self.reflection.to_owned_material(),
			refraction: self.refraction.to_owned_material(),
			refractive_index: self.refractive_index,
			dispersion: self.dispersion
		} as ~Material: Send+Freeze
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

	pub fn from_json(config: &~json::Object) -> Option<~Material: Send+Freeze> {
		let label = match config.find(&~"label") {
			Some(&json::String(ref label)) => label.to_owned(),
			_ => ~"<FresnelMix>"
		};

		let refractive_index = match config.find(&~"ior") {
			Some(&json::Number(refractive_index)) => refractive_index as f32,
			_ => {
				println!("Warning: Index of refraction for {} is not set", label);
				1.0
			}
		};

		let dispersion = match config.find(&~"dispersion") {
			Some(&json::Number(dispersion)) => dispersion as f32,
			None => 0.0,
			_ => {
				println!("Warning: Invalid dispersion factor for {}", label);
				0.0
			}
		};


		let reflection = match config.find(&~"reflection") {
			Some(&json::Object(ref material)) => from_json(material),
			_ => {
				println!("Warning: Invalid reflection material for {}", label);
				None
			}
		};

		if reflection.is_none() {
			return None;
		}

		let refraction = match config.find(&~"refraction") {
			Some(&json::Object(ref material)) => from_json(material),
			_ => {
				println!("Warning: Invalid refraction material for {}", label);
				None
			}
		};

		if refraction.is_none() {
			return None;
		}

		Some(~FresnelMix {
			reflection: reflection.unwrap(),
			refraction: refraction.unwrap(),
			refractive_index: refractive_index,
			dispersion: dispersion
		} as ~Material: Send+Freeze)
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

	fn to_owned_material(&self) -> ~Material: Send+Freeze {
		~Refractive {
			color: self.color,
			refractive_index: self.refractive_index,
			dispersion: self.dispersion
		} as ~Material: Send+Freeze
	}
}

impl Refractive {
	pub fn from_json(config: &~json::Object) -> Option<~Material: Send+Freeze> {
		let label = match config.find(&~"label") {
			Some(&json::String(ref label)) => label.to_owned(),
			_ => ~"<Refractive>"
		};

		let color = match config.find(&~"color") {
			Some(&json::Number(color)) => color as f32,
			_ => {
				println!("Warning: Color for {} is not set", label);
				1.0
			}
		};

		let refractive_index = match config.find(&~"ior") {
			Some(&json::Number(refractive_index)) => refractive_index as f32,
			_ => {
				println!("Warning: Index of refraction for {} is not set", label);
				1.0
			}
		};

		let dispersion = match config.find(&~"dispersion") {
			Some(&json::Number(dispersion)) => dispersion as f32,
			None => 0.0,
			_ => {
				println!("Warning: Invalid dispersion factor for {}", label);
				0.0
			}
		};

		Some(~Refractive {
			color: color,
			refractive_index: refractive_index,
			dispersion: dispersion
		} as ~Material: Send+Freeze)
	}
}