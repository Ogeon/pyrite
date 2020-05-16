use cgmath::Vector3;
use cgmath::{EuclideanSpace, Point3, Quaternion};
use cgmath::{Matrix4, SquareMatrix};

use crate::config::entry::Entry;
use crate::config::Prelude;

pub fn register_types(context: &mut Prelude) {
    {
        let mut object = context.object("Vector".into());
        object.add_decoder(decode_vector_3d);
        object.add_decoder(decode_point_3d);
        object.add_decoder(decode_quaternion);
        object.arguments(vec!["x".into(), "y".into(), "z".into(), "w".into()]);
    }
    context
        .object("Transform".into())
        .object("LookAt".into())
        .add_decoder(decode_transform_look_at);
}

fn decode_vector_3d(entry: Entry<'_>) -> Result<Vector3<f32>, String> {
    let items = entry.as_object().ok_or("not an object")?;

    let x = match items.get("x") {
        Some(v) => try_for!(v.decode(), "x"),
        None => 0.0,
    };

    let y = match items.get("y") {
        Some(v) => try_for!(v.decode(), "y"),
        None => 0.0,
    };

    let z = match items.get("z") {
        Some(v) => try_for!(v.decode(), "z"),
        None => 0.0,
    };

    Ok(Vector3::new(x, y, z))
}

fn decode_point_3d(entry: Entry<'_>) -> Result<Point3<f32>, String> {
    let items = entry.as_object().ok_or("not an object")?;

    let x = match items.get("x") {
        Some(v) => try_for!(v.decode(), "x"),
        None => 0.0,
    };

    let y = match items.get("y") {
        Some(v) => try_for!(v.decode(), "y"),
        None => 0.0,
    };

    let z = match items.get("z") {
        Some(v) => try_for!(v.decode(), "z"),
        None => 0.0,
    };

    Ok(Point3::new(x, y, z))
}

fn decode_quaternion(entry: Entry<'_>) -> Result<Quaternion<f32>, String> {
    let items = entry.as_object().ok_or("not an object")?;

    let x = match items.get("x") {
        Some(v) => try_for!(v.decode(), "x"),
        None => 0.0,
    };

    let y = match items.get("y") {
        Some(v) => try_for!(v.decode(), "y"),
        None => 0.0,
    };

    let z = match items.get("z") {
        Some(v) => try_for!(v.decode(), "z"),
        None => 0.0,
    };

    let w = match items.get("w") {
        Some(v) => try_for!(v.decode(), "w"),
        None => 0.0,
    };

    Ok(Quaternion::new(x, y, z, w))
}

fn decode_transform_look_at(entry: Entry<'_>) -> Result<Matrix4<f32>, String> {
    let items = entry.as_object().ok_or("not an object")?;

    let from = match items.get("from") {
        Some(v) => try_for!(v.dynamic_decode(), "from"),
        None => Vector3::new(0.0, 0.0, 0.0),
    };

    let to = match items.get("to") {
        Some(v) => try_for!(v.dynamic_decode(), "to"),
        None => Vector3::new(0.0, 0.0, 0.0),
    };

    let up = match items.get("up") {
        Some(v) => try_for!(v.dynamic_decode(), "up"),
        None => Vector3::new(0.0, 1.0, 0.0),
    };

    Matrix4::look_at(Point3::from_vec(from), Point3::from_vec(to), up)
        .invert()
        .map(|m| Ok(m))
        .unwrap_or(Err("could not invert view matrix".into()))
}
