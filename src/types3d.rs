use std::collections::HashMap;

use cgmath::matrix::Matrix4;
use cgmath::vector::Vector3;
use cgmath::point::Point;

use config;
use config::FromConfig;

pub fn register_types(context: &mut config::ConfigContext) {
    context.insert_type("Vector", "3D", decode_vector_3d);
    context.insert_type("Transform", "LookAt", decode_transform_look_at);
}

fn decode_vector_3d(_context: &config::ConfigContext, items: HashMap<String, config::ConfigItem>) -> Result<Vector3<f64>, String> {
    let mut items = items;

    let x = match items.pop_equiv(&"x") {
        Some(v) => try!(FromConfig::from_config(v), "x"),
        None => 0.0
    };

    let y = match items.pop_equiv(&"y") {
        Some(v) => try!(FromConfig::from_config(v), "y"),
        None => 0.0
    };

    let z = match items.pop_equiv(&"z") {
        Some(v) => try!(FromConfig::from_config(v), "z"),
        None => 0.0
    };

    Ok(Vector3::new(x, y, z))

}

fn decode_transform_look_at(context: &config::ConfigContext, items: HashMap<String, config::ConfigItem>) -> Result<Matrix4<f64>, String> {
    let mut items = items;

    let from = match items.pop_equiv(&"from") {
        Some(v) => try!(context.decode_structure_of_type("Vector", "3D", v), "from"),
        None => Vector3::new(0.0, 0.0, 0.0)
    };

    let to = match items.pop_equiv(&"to") {
        Some(v) => try!(context.decode_structure_of_type("Vector", "3D", v), "to"),
        None => Vector3::new(0.0, 0.0, 0.0)
    };

    let up = match items.pop_equiv(&"up") {
        Some(v) => try!(context.decode_structure_of_type("Vector", "3D", v), "up"),
        None => Vector3::new(0.0, 1.0, 0.0)
    };

    Ok(Matrix4::look_at(&Point::from_vec(&from), &Point::from_vec(&to), &up))
}