use extra::json;
use core::ParametricValue;

pub fn from_json(config: &json::Json) -> Option<~ParametricValue: Send+Freeze> {
	match config {
		&json::Number(num) => {
			Some(~Number{value: num as f32} as ~ParametricValue: Send+Freeze)
		},
		&json::Object(ref value_cfg) => {
			match value_cfg.find(&~"type") {
				Some(&json::String(~"Echo")) => {
					Some(~Echo{a: 0.0} as ~ParametricValue: Send+Freeze)
				},
				Some(&json::String(~"Add")) => {
					Add::from_json(value_cfg)
				},
				Some(&json::String(~"Multiply")) => {
					Multiply::from_json(value_cfg)
				},
				_ => None
			}
		},
		_ => None
	}
}


//A plain number
struct Number {
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
struct Add {
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
struct Multiply {
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



//Test
struct Echo {
    a: f32
}

impl ParametricValue for Echo {
	fn get(&self, _: f32, _: f32, i: f32) -> f32 {
		i
	}

	fn clone_value(&self) -> ~ParametricValue: Send+Freeze {
		~Echo{a: self.a} as ~ParametricValue: Send+Freeze
	}
}