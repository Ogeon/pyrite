use std::iter::range;
use std::vec;
use extra::json;
use core::ParametricValue;

pub fn from_json(config: &json::Json) -> Option<~ParametricValue: Send+Freeze> {
	match config {
		&json::Number(num) => {
			Some(~Number{value: num as f32} as ~ParametricValue: Send+Freeze)
		},
		&json::Object(ref value_cfg) => {
			match value_cfg.find(&~"type") {
				Some(&json::String(~"Add")) => {
					Add::from_json(value_cfg)
				},
				Some(&json::String(~"Multiply")) => {
					Multiply::from_json(value_cfg)
				},
				Some(&json::String(~"Curve")) => {
					Curve::from_json(value_cfg)
				},
				_ => None
			}
		},
		_ => None
	}
}


pub fn make_image(value: &ParametricValue, from: f32, to: f32, width: uint, height: uint) -> ~[u8] {
	let diff = to - from;

	let values: ~[f32] = range(0, width).map(|i| {
		value.get(0.0, 0.0, from + diff * (i as f32 / width as f32))
	}).collect();


	vec::from_fn(height, |y| {
		let lim = 1.0 - (y as f32 / height as f32);
		vec::from_fn(width, |x| {
			let pos = from + (x as f32 / width as f32) * diff;
			if values[x] >= lim {
				if (pos / 10.0).floor() % 2.0 == 0.0 {
					~[128u8, 128u8, 128u8]
				} else {
					~[100u8, 100u8, 100u8]
				}
			} else {
				if (pos / 10.0).floor() % 2.0 == 0.0 {
					~[16u8, 16u8, 16u8]
				} else {
					~[64u8, 64u8, 64u8]
				}
			}
		}).concat_vec()
	}).concat_vec()
}


//A plain number
pub struct Number {
	value: f32
}

impl ParametricValue for Number {
	fn get(&self, _: f32, _: f32, _: f32) -> f32 {
		self.value
	}

	fn clone_value(&self) -> ~ParametricValue: Send+Freeze {
		~Number{value: self.value} as ~ParametricValue: Send+Freeze
	}
}



//Add
pub struct Add {
	value_a: ~ParametricValue: Send+Freeze,
	value_b: ~ParametricValue: Send+Freeze
}

impl ParametricValue for Add {
	fn get(&self, x: f32, y: f32, i: f32) -> f32 {
		self.value_a.get(x, y, i) + self.value_b.get(x, y, i)
	}

	fn clone_value(&self) -> ~ParametricValue: Send+Freeze {
		~Add{
			value_a: self.value_a.clone_value(),
			value_b: self.value_b.clone_value()
		} as ~ParametricValue: Send+Freeze
	}
}

impl Add {
	fn from_json(config: &~json::Object) -> Option<~ParametricValue: Send+Freeze> {
		let a = match config.find(&~"value_a") {
			Some(value) => from_json(value),
			None => None
		};

		if a.is_none() {
			return None;
		}

		let b = match config.find(&~"value_b") {
			Some(value) => from_json(value),
			None => None
		};

		if b.is_none() {
			return None;
		}


		return Some(~Add {
			value_a: a.unwrap(),
			value_b: b.unwrap()
		} as ~ParametricValue: Send+Freeze);
	}
}



//Multiply
pub struct Multiply {
	value_a: ~ParametricValue: Send+Freeze,
	value_b: ~ParametricValue: Send+Freeze
}

impl ParametricValue for Multiply {
	fn get(&self, x: f32, y: f32, i: f32) -> f32 {
		self.value_a.get(x, y, i) * self.value_b.get(x, y, i)
	}

	fn clone_value(&self) -> ~ParametricValue: Send+Freeze {
		~Multiply{
			value_a: self.value_a.clone_value(),
			value_b: self.value_b.clone_value()
		} as ~ParametricValue: Send+Freeze
	}
}

impl Multiply {
	fn from_json(config: &~json::Object) -> Option<~ParametricValue: Send+Freeze> {
		let a = match config.find(&~"value_a") {
			Some(value) => from_json(value),
			None => None
		};

		if a.is_none() {
			return None;
		}

		let b = match config.find(&~"value_b") {
			Some(value) => from_json(value),
			None => None
		};

		if b.is_none() {
			return None;
		}


		return Some(~Multiply {
			value_a: a.unwrap(),
			value_b: b.unwrap()
		} as ~ParametricValue: Send+Freeze);
	}
}



//Response curve
pub struct Curve {
	points: ~[(f32, f32)],
	y_prim: ~[f32]
}

impl ParametricValue for Curve {
	fn get(&self, _: f32, _: f32, i: f32) -> f32 {
		if self.points.len() == 0 {
			0.0
		} else if self.points.len() == 1 {
			let (_, y) = self.points[0];
			y
		} else {
			let (min_x, min_y) = self.points[0];
			let &(max_x, max_y) = self.points.last();

			if i < min_x {
				min_y
			} else if i > max_x {
				max_y
			} else {
				let mut klo = 0;
				let mut khi = self.points.len() - 1;

				while khi - klo > 1 {
					let k = (khi + klo) / 2;
					let (x, _) = self.points[k];
					if x > i {
						khi = k;
					} else {
						klo = k;
					}
				}

				let (x_hi, y_hi) = self.points[khi];
				let (x_lo, y_lo) = self.points[klo];
				let h = x_hi - x_lo;
				let a = (x_hi - i) / h;
				let b = (i - x_lo) / h;

				a * y_lo + b * y_hi + ((a*a*a - a) * self.y_prim[klo] + (b*b*b - b) * self.y_prim[khi]) * (h * h) / 6.0
			}
		}
	}

	fn clone_value(&self) -> ~ParametricValue: Send+Freeze {
		~Curve{
			points: self.points.clone(),
			y_prim: self.y_prim.clone()
		} as ~ParametricValue: Send+Freeze
	}
}

impl Curve {
	pub fn init(points: ~[(f32, f32)]) -> Curve {
		let mut u = vec::from_elem(points.len(), 0.0f32);
		let mut y2 = vec::from_elem(points.len(), 0.0f32);

		for i in range(1, points.len() - 1) {
			let (x_prev, y_prev) = points[i-1];
			let (x, y) = points[i];
			let (x_next, y_next) = points[i+1];

			let sig = (x - x_prev) / (x_next - x_prev);
			let p = sig * y2[i-1] + 2.0;
			let q = (y_next - y) / (x_next - x) - (y - y_prev) / (x - x_prev);

			y2[i] = (sig - 1.0) / p;
			u[i] = (6.0 * q / (x_next - x_prev) - sig * u[i-1]) / p;
		}

		for i in range(0, points.len() - 1).invert() {
			y2[i] = y2[i] * y2[i+1] + u[i];
		}

		Curve {
			points: points,
			y_prim: y2
		}
	}

	fn from_json(config: &~json::Object) -> Option<~ParametricValue: Send+Freeze> {
		let points = match config.find(&~"points") {
			Some(&json::List(ref list)) => {
				list.iter().filter_map(|v| {
					match v {
						&json::List([json::Number(x), json::Number(y)]) => {
							Some((x as f32, y as f32))
						},
						_ => None
					}
				}).collect()
			},
			_ => ~[]
		};

		Some(~Curve::init(points) as ~ParametricValue: Send+Freeze)
	}
}