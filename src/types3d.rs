use cgmath::{Matrix, Matrix4};
use cgmath::Vector3;
use cgmath::Point;

use config::Prelude;
use config::entry::Entry;

pub fn register_types(context: &mut Prelude) {
    context.object("Vector".into()).add_decoder(decode_vector_3d);
    context.object("Transform".into()).object("LookAt".into()).add_decoder(decode_transform_look_at);
}

fn decode_vector_3d(entry: Entry) -> Result<Vector3<f64>, String> {
    let items = try!(entry.as_object().ok_or("not an object".into()));

    let x = match items.get("x") {
        Some(v) => try!(v.decode(), "x"),
        None => 0.0
    };

    let y = match items.get("y") {
        Some(v) => try!(v.decode(), "y"),
        None => 0.0
    };

    let z = match items.get("z") {
        Some(v) => try!(v.decode(), "z"),
        None => 0.0
    };

    Ok(Vector3::new(x, y, z))

}

fn decode_transform_look_at(entry: Entry) -> Result<Matrix4<f64>, String> {
    let items = try!(entry.as_object().ok_or("not an object".into()));

    let from = match items.get("from") {
        Some(v) => try!(v.dynamic_decode(), "from"),
        None => Vector3::new(0.0, 0.0, 0.0)
    };

    let to = match items.get("to") {
        Some(v) => try!(v.dynamic_decode(), "to"),
        None => Vector3::new(0.0, 0.0, 0.0)
    };

    let up = match items.get("up") {
        Some(v) => try!(v.dynamic_decode(), "up"),
        None => Vector3::new(0.0, 1.0, 0.0)
    };

    Matrix4::look_at(&Point::from_vec(&from), &Point::from_vec(&to), &up).invert().map(|m| Ok(m)).unwrap_or(Err("could not invert view matrix".into()))
}