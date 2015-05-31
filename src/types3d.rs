use std::collections::HashMap;

use cgmath::{Matrix, Matrix4};
use cgmath::Vector3;
use cgmath::Point;

use config;
use config::{FromConfig, Type};

pub fn register_types(context: &mut config::ConfigContext) {
    context.insert_type("Vector", decode_vector_3d);
    context.insert_grouped_type("Transform", "LookAt", decode_transform_look_at);
}

fn decode_vector_3d(_context: &config::ConfigContext, items: HashMap<String, config::ConfigItem>) -> Result<Vector3<f64>, String> {
    let mut items = items;

    let x = match items.remove("x") {
        Some(v) => try!(FromConfig::from_config(v), "x"),
        None => 0.0
    };

    let y = match items.remove("y") {
        Some(v) => try!(FromConfig::from_config(v), "y"),
        None => 0.0
    };

    let z = match items.remove("z") {
        Some(v) => try!(FromConfig::from_config(v), "z"),
        None => 0.0
    };

    Ok(Vector3::new(x, y, z))

}

fn decode_transform_look_at(context: &config::ConfigContext, items: HashMap<String, config::ConfigItem>) -> Result<Matrix4<f64>, String> {
    let mut items = items;

    let from = match items.remove("from") {
        Some(v) => try!(context.decode_structure_of_type(&Type::single("Vector"), v), "from"),
        None => Vector3::new(0.0, 0.0, 0.0)
    };

    let to = match items.remove("to") {
        Some(v) => try!(context.decode_structure_of_type(&Type::single("Vector"), v), "to"),
        None => Vector3::new(0.0, 0.0, 0.0)
    };

    let up = match items.remove("up") {
        Some(v) => try!(context.decode_structure_of_type(&Type::single("Vector"), v), "up"),
        None => Vector3::new(0.0, 1.0, 0.0)
    };

    Matrix4::look_at(&Point::from_vec(&from), &Point::from_vec(&to), &up).invert().map(|m| Ok(m)).unwrap_or(Err("could not invert view matrix".into()))
}