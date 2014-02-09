extern mod std;
use std::vec;
use extra::json;
use nalgebra::na;
use nalgebra::na::Vec3;
use core::{Ray, Material, RandomVariable, Reflection, ParametricValue};
use values;


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
			println!("Error: Unknown material type \"{}\".", something.to_owned());
			None
		},

		_ => None
	}
}


pub fn get_value(config: &~json::Object, default: f32, key: ~str, label: &str) -> ~ParametricValue: Send+Freeze {
	match config.find(&key) {
		Some(value_cfg) => match values::from_json(value_cfg) {
			Some(value) => value,
			None => {
				println!("Warning: \"{}\" for material \"{}\" must be a parmetric value. Default will be used.", key, label);
				~values::Number{value: default} as ~ParametricValue: Send+Freeze
			}
		},
		None => {
			~values::Number{value: default} as ~ParametricValue: Send+Freeze
		}
	}
}


//Diffuse
pub struct Diffuse {
	color: ~ParametricValue: Send+Freeze
}

impl Material for Diffuse {
	fn get_reflection(&self, normal: Ray, ray_in: Ray, _: f32, rand_var: &mut RandomVariable) -> Reflection {
		let u = rand_var.next();
		let v = rand_var.next();
		let theta = 2.0 * std::f32::consts::PI * u;
		let phi = std::num::acos(2.0 * v - 1.0);
		let sphere_point = Vec3::new(
			phi.sin() * theta.cos(),
			phi.sin() * theta.sin(),
			phi.cos().abs()
			);

		let n = if na::dot(&ray_in.direction, &normal.direction) < 0.0 {
			normal.direction
		} else {
			-normal.direction
		};

		let mut bases = vec::with_capacity(3);

		na::orthonormal_subspace_basis(&n, |base| {
			bases.push(base);
			true
		});
		bases.push(n);

		let mut reflection: Vec3<f32> = na::zero();

		unsafe {
			for (i, base) in bases.iter().enumerate() {
				reflection = reflection + base * sphere_point.at_fast(i);
			}
		}

		Reflection {
			out: Ray::new(normal.origin, reflection),
			color: self.color,
			emission: false,
			dispersion: false
		}
	}
}

impl Diffuse {
	pub fn from_json(config: &~json::Object) -> Option<~Material: Send+Freeze> {
		let label = match config.find(&~"label") {
			Some(&json::String(ref label)) => label.to_owned(),
			_ => ~"<Diffuse>"
		};

		let color = get_value(config, 1.0, ~"color", label);

		Some(~Diffuse {
			color: color
		} as ~Material: Send+Freeze)
	}
}


//Mirror
pub struct Mirror {
	color: ~ParametricValue: Send+Freeze
}

impl Material for Mirror {
	fn get_reflection(&self, normal: Ray, ray_in: Ray, _: f32, _: &mut RandomVariable) -> Reflection {
		let perp = na::dot(&ray_in.direction, &normal.direction) * 2.0;
		Reflection {
			out: Ray::new(normal.origin, ray_in.direction - (normal.direction * perp)),
			color: self.color,
			emission: false,
			dispersion: false
		}
	}
}

impl Mirror {
	pub fn from_json(config: &~json::Object) -> Option<~Material: Send+Freeze> {
		let label = match config.find(&~"label") {
			Some(&json::String(ref label)) => label.to_owned(),
			_ => ~"<Mirror>"
		};

		let color = get_value(config, 1.0, ~"color", label);

		Some(~Mirror {
			color: color
		} as ~Material: Send+Freeze)
	}
}


//Emission
pub struct Emission {
    color: ~ParametricValue: Send+Freeze,
    luminance: f32
}

impl Material for Emission {
	fn get_reflection(&self, _: Ray, _: Ray, _: f32, _: &mut RandomVariable) -> Reflection {
		Reflection {
			out: Ray::new(na::zero(), na::zero()),
			color: self.color,
			emission: true,
			dispersion: false
		}
	}
}

impl Emission {
	pub fn from_json(config: &~json::Object) -> Option<~Material: Send+Freeze> {
		let label = match config.find(&~"label") {
			Some(&json::String(ref label)) => label.to_owned(),
			_ => ~"<Emission>"
		};

		let color = get_value(config, 1.0, ~"color", label);

		let luminance = match config.find(&~"luminance") {
			Some(&json::Number(luminance)) => luminance as f32,
			None => {
				1.0
			},
			_ => {
				println!("Warning: \"luminance\" for material \"{}\" must be a number. Default will be used.", label);
				1.0
			}
		};

		Some(~Emission {
			color: ~values::Multiply {
				value_a: color,
				value_b: ~values::Number{value: luminance} as ~ParametricValue:Send+Freeze
			} as ~ParametricValue:Send+Freeze,
			luminance: luminance
		} as ~Material: Send+Freeze)
	}
}


//Mix
pub struct Mix {
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

impl Mix {
	pub fn from_json(config: &~json::Object) -> Option<~Material: Send+Freeze> {
		let label = match config.find(&~"label") {
			Some(&json::String(ref label)) => label.to_owned(),
			_ => ~"<Mix>"
		};

		let factor = match config.find(&~"factor") {
			Some(&json::Number(factor)) => factor as f32,
			None => {
				0.5
			},
			_ => {
				println!("Warning: \"factor\" for material \"{}\" must be a number. Default will be used.", label);
				0.5
			}
		};

		let material_a = match config.find(&~"material_a") {
			Some(&json::Object(ref material)) => from_json(material),
			None => {
				println!("Error: \"material_a\" for material \"{}\" is not set.", label);
				None
			},
			_ => {
				println!("Error: \"material_a\" for material \"{}\" must be an object.", label);
				None
			}
		};

		if material_a.is_none() {
			return None;
		}

		let material_b = match config.find(&~"material_b") {
			Some(&json::Object(ref material)) => from_json(material),
			None => {
				println!("Error: \"material_b\" for material \"{}\" is not set.", label);
				None
			},
			_ => {
				println!("Error: \"material_b\" for material \"{}\" must be an object.", label);
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
pub struct FresnelMix {
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

	pub fn from_json(config: &~json::Object) -> Option<~Material: Send+Freeze> {
		let label = match config.find(&~"label") {
			Some(&json::String(ref label)) => label.to_owned(),
			_ => ~"<FresnelMix>"
		};

		let refractive_index = match config.find(&~"ior") {
			Some(&json::Number(refractive_index)) => refractive_index as f32,
			None => {
				0.5
			},
			_ => {
				println!("Warning: \"ior\" for material \"{}\" must be a number. Default will be used.", label);
				0.5
			}
		};

		let dispersion = match config.find(&~"dispersion") {
			Some(&json::Number(dispersion)) => dispersion as f32,
			None => {
				0.0
			},
			_ => {
				println!("Warning: \"dispersion\" for material \"{}\" must be a number. Default will be used.", label);
				0.0
			}
		};


		let reflection = match config.find(&~"reflection") {
			Some(&json::Object(ref material)) => from_json(material),
			None => {
				println!("Error: \"reflection\" for material \"{}\" is not set.", label);
				None
			},
			_ => {
				println!("Error: \"reflection\" for material \"{}\" must be an object.", label);
				None
			}
		};

		if reflection.is_none() {
			return None;
		}

		let refraction = match config.find(&~"refraction") {
			Some(&json::Object(ref material)) => from_json(material),
			None => {
				println!("Error: \"refraction\" for material \"{}\" is not set.", label);
				None
			},
			_ => {
				println!("Error: \"refraction\" for material \"{}\" must be an object.", label);
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
pub struct Refractive {
	color: ~ParametricValue: Send+Freeze,
	refractive_index: f32,
	dispersion: f32
}

impl Material for Refractive {
	fn get_reflection(&self, normal: Ray, ray_in: Ray, frequency: f32, rand_var: &mut RandomVariable) -> Reflection {
		let perp = na::dot(&ray_in.direction, &normal.direction) * 2.0;
		let refl_dir = ray_in.direction - (normal.direction * perp);

		let ior = self.refractive_index + self.dispersion / frequency;
		let nl = if na::dot(&normal.direction, &ray_in.direction) < 0.0 {normal.direction} else {-normal.direction};
		let into = na::dot(&normal.direction, &nl) > 0.0;
		let nc = 1.0;
		let nnt = if into {nc / ior} else {ior / nc};
		let ddn = na::dot(&ray_in.direction, &nl);
		let cos2t = 1.0 - nnt * nnt * (1.0 - ddn * ddn);

		if cos2t < 0.0 {
			return Reflection {
				out: Ray::new(normal.origin, refl_dir),
				color: ~values::Number{value: 1.0} as ~ParametricValue: Send + Freeze,
				emission: false,
				dispersion: self.dispersion != 0.0
			}
		}

		let tvec = ray_in.direction * nnt - normal.direction * (if into {1.0} else {-1.0} * (ddn * nnt + cos2t.sqrt()));
		let tdir = na::normalize(&tvec);
		let a = ior - 1.0;
		let b = ior + 1.0;
		let R0 = (a * a) / (b * b);
		let c = 1.0 - if into {-ddn} else {na::dot(&tdir, &normal.direction)};
		let Re = R0 + (1.0 - R0) * c * c * c * c * c;
		let Tr = 1.0 - Re;
		let P = 0.25 + 0.5 * Re;
		let RP = Re / P;
		let TP = Tr / (1.0 - P);

		if rand_var.next() < P {
			return Reflection {
				out: Ray::new(normal.origin, refl_dir),
				color: values::Number{value: RP}.clone_value(),
				emission: false,
				dispersion: self.dispersion != 0.0
			}
		} else {
			return Reflection {
				out: Ray::new(normal.origin, tdir),
				color: ~values::Multiply {
					value_a: self.color.clone_value(),
					value_b: ~values::Number{value: TP} as ~ParametricValue: Send + Freeze
				} as ~ParametricValue,
				emission: false,
				dispersion: self.dispersion != 0.0
			}
		}
	}
}

impl Refractive {
	pub fn from_json(config: &~json::Object) -> Option<~Material: Send+Freeze> {
		let label = match config.find(&~"label") {
			Some(&json::String(ref label)) => label.to_owned(),
			_ => ~"<Refractive>"
		};

		let color = get_value(config, 1.0, ~"color", label);

		let refractive_index = match config.find(&~"ior") {
			Some(&json::Number(refractive_index)) => refractive_index as f32,
			None => {
				1.0
			},
			_ => {
				println!("Warning: \"ior\" for material \"{}\" must be a number. Default will be used.", label);
				1.0
			}
		};

		let dispersion = match config.find(&~"dispersion") {
			Some(&json::Number(dispersion)) => dispersion as f32,
			None => {
				0.0
			},
			_ => {
				println!("Warning: \"dispersion\" for material \"{}\" must be a number. Default will be used.", label);
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