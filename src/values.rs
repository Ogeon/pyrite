use extra::json;
use core::ParametricValue;

pub fn from_json(config: &json::Json) -> Option<~ParametricValue: Send+Freeze> {
	match config {
		json::Number(num) => Some(~Number{value: num as f32} as ~ParametricValue: Send+Freeze),
		_ => {
			println!("Warning: Parametric value must be a number or an object.", label);
			None
		}
	}
}


//A plain number
struct Number {
	value: f32
}

impl ParametricValue for Number {
	fn get(&self, x: f32, y: f32, i: f32) -> f32 {
		self.value
	}

	fn clone_value(&self) -> ~ParametricValue {
		Number{value: self.value} as ~ParametricValue
	}
}