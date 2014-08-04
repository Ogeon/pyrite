use std::collections::HashMap;
use std::any::{Any, AnyRefExt};

mod parser;
mod interpreter;

pub struct ConfigContext {
	groups: HashMap<String, HashMap<String, Box<Any>>>
}

impl ConfigContext {
	pub fn new() -> ConfigContext {
		ConfigContext {
			groups: HashMap::new()
		}
	}

	pub fn insert_type<T, D: 'static + Decoder<T>>(&mut self, group_name: String, type_name: String, decoder: D) -> bool {
		self.groups.find_or_insert_with(group_name, |_| HashMap::new()).insert(type_name, box decoder as Box<Any>)
	}

	pub fn decode<T: FromConfig>(&self, item: ConfigItem) -> Result<T, String> {
		match item {
			Structure(Some((group_name, type_name)), fields) => self.decode_structure(group_name, type_name, fields),
			Structure(None, fields) => FromConfig::from_structure(None, fields),
			Primitive(value) => FromConfig::from_primitive(value)
		}
	}

	pub fn decode_structure_from_group<T>(&self, group_name: String, item: ConfigItem) -> Result<T, String> {
		match item {
			Structure(Some((item_group_name, type_name)), fields) => if group_name == item_group_name {
				self.decode_structure(group_name, type_name, fields)
			} else {
				Err(format!("expected a structure from group {}.*, but found {}.{}", group_name, item_group_name, type_name))
			},
			Structure(None, _) => Err(format!("expected a structure from group {}.*, but found an untyped structure", group_name)),
			Primitive(value) => Err(format!("expected a structure from group {}.*, but found {}", group_name, value))
		}
	}

	pub fn decode_structure<T>(&self, group_name: String, type_name: String, fields: HashMap<String, ConfigItem>) -> Result<T, String> {
		match self.groups.find(&group_name).and_then(|group| group.find(&type_name)) {
			Some(decoder) => match decoder.downcast_ref::<Box<Decoder<T> + 'static>>() {
				Some(decoder) => decoder.decode(self, fields),
				None => Err(format!("type cannot be decoded from {}.{}", group_name, type_name))
			},
			None => Err(format!("unknown type {}.{}", group_name, type_name))
		}
	}
}



trait Decoder<T> {
	fn decode(&self, context: &ConfigContext, fields: HashMap<String, ConfigItem>) -> Result<T, String>;
}

impl<T> Decoder<T> for fn(&ConfigContext, HashMap<String, ConfigItem>) -> Result<T, String> {
	fn decode(&self, context: &ConfigContext, fields: HashMap<String, ConfigItem>) -> Result<T, String> {
		(*self)(context, fields)
	}
}



pub enum ConfigItem {
	Structure(Option<(String, String)>, HashMap<String, ConfigItem>),
	Primitive(parser::Value)
}

impl ConfigItem {
	pub fn into_float(self) -> Option<f64> {
		match self {
			Primitive(parser::Number(f)) => Some(f),
			_ => None
		}
	}

	pub fn is_float(&self) -> bool {
		match self {
			&Primitive(parser::Number(_)) => true,
			_ => false
		}
	}

	pub fn into_string(self) -> Option<String> {
		match self {
			Primitive(parser::String(s)) => Some(s),
			_ => None
		}
	}

	pub fn is_string(&self) -> bool {
		match self {
			&Primitive(parser::String(_)) => true,
			_ => false
		}
	}

	pub fn into_fields(self) -> Option<HashMap<String, ConfigItem>> {
		match self {
			Structure(_, fields) => Some(fields),
			_ => None
		}
	}

	pub fn is_structure(&self) -> bool {
		match self {
			&Structure(..) => true,
			_ => false
		}
	}
}

pub trait FromConfig {
	fn from_primitive(item: parser::Value) -> Result<Self, String> {
		Err(format!("unexpected {}", item))
	}

	fn from_structure(structure_type: Option<(String, String)>, fields: HashMap<String, ConfigItem>) -> Result<Self, String> {
		match structure_type {
			Some((group_name, type_name)) => Err(format!("unexpected structure of type {}.{}", group_name, type_name)),
			None => Err(String::from_str("unexpected untyped structure"))
		}
	}
}

impl FromConfig for f64 {
	fn from_primitive(item: parser::Value) -> Result<f64, String> {
		match item {
			parser::Number(f) => Ok(f),
			_ => Err(String::from_str("expected a number"))
		}
	}
}

impl FromConfig for f32 {
	fn from_primitive(item: parser::Value) -> Result<f32, String> {
		match item {
			parser::Number(f) => Ok(f as f32),
			_ => Err(String::from_str("expected a number"))
		}
	}
}

impl FromConfig for String {
	fn from_primitive(item: parser::Value) -> Result<String, String> {
		match item {
			parser::String(s) => Ok(s),
			_ => Err(String::from_str("expected a string"))
		}
	}
}