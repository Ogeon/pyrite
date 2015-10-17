use std;
use std::sync::Arc;

use cgmath::{Vector, EuclideanVector, Vector3, Point, Point3, Ray3};

use rand::Rng;

use config::Prelude;
use config::entry::Entry;

use tracer::{self, FloatRng, Material, ParametricValue, RenderContext};
use shapes::Shape;
use world;

pub enum Lamp {
    Directional {
        direction: Vector3<f64>,
        width: f64,
        color: Box<ParametricValue<RenderContext, f64>>
    },
    Point(Point3<f64>, Box<ParametricValue<RenderContext, f64>>),
    Shape(Arc<Shape>)
}

fn ortho(v: &Vector3<f64>) -> Vector3<f64> {
    let unit = if v.x.abs() < 0.00001 {
        Vector3::unit_x()
    } else if v.y.abs() < 0.00001 {
        Vector3::unit_y()
    } else if v.z.abs() < 0.00001 {
        Vector3::unit_z()
    } else {
        Vector3 {
            x: -v.y,
            y: v.x,
            z: 0.0
        }
    };

    v.cross(&unit)
}

impl Lamp {
    pub fn sample<R: Rng + FloatRng>(&self, rng: &mut R, target: Point3<f64>) -> Sample {
        match *self {
            Lamp::Directional { direction, width, ref color } => {
                let o1 = ortho(&direction).normalize();
                let o2 = direction.cross(&o1).normalize();
                let r1: f64 = std::f64::consts::PI * 2.0 * rng.gen::<f64>();
                let r2: f64 = width + (1.0 - width) * rng.gen::<f64>();
                let oneminus = (1.0f64 - r2 * r2).sqrt();

                let dir = o1.mul_s(r1.cos() * oneminus).add_v(&o2.mul_s(r1.sin() * oneminus)).add_v(&direction.mul_s(r2));

                Sample {
                    direction: dir,
                    sq_distance: None,
                    surface: Surface::Color(&**color)
                }
            },
            Lamp::Point(ref center, ref color) => {
                let v = center.sub_p(&target);
                let distance = v.length2();
                Sample {
                    direction: v.normalize(),
                    sq_distance: Some(distance),
                    surface: Surface::Color(&**color)
                }
            },
            Lamp::Shape(ref shape) => {
                let ray = shape.sample_point(rng).expect("trying to use infinite shape in direct lighting");
                let v = ray.origin.sub_p(&target);
                let distance = v.length2();
                Sample {
                    direction: v.normalize(),
                    sq_distance: Some(distance),
                    surface: Surface::Physical {
                        normal: ray,
                        material: shape.get_material()
                    }
                }
            }
        }
    }

    pub fn surface_area(&self) -> f64 {
        match *self {
            Lamp::Directional { .. } => 1.0,
            Lamp::Point(_, _) => 4.0 * std::f64::consts::PI,
            Lamp::Shape(ref shape) => shape.surface_area()
        }
    }
}

pub struct Sample<'a> {
    pub direction: Vector3<f64>,
    pub sq_distance: Option<f64>,
    pub surface: Surface<'a>
}

pub enum Surface<'a> {
    Physical {
        normal: Ray3<f64>,
        material: &'a Material
    },
    Color(&'a ParametricValue<RenderContext, f64>),
}

pub fn register_types(context: &mut Prelude) {
    let mut group = context.object("Light".into());
    group.object("Directional".into()).add_decoder(decode_directional);
    group.object("Point".into()).add_decoder(decode_point);
}

fn decode_directional(entry: Entry) -> Result<world::Object, String> {
    let fields = try!(entry.as_object().ok_or("not an object".into()));

    let direction: Vector3<_> = match fields.get("direction") {
        Some(v) => try!(v.dynamic_decode(), "direction"),
        None => return Err("missing field 'direction'".into())
    };

    let width: f64 = match fields.get("width") {
        Some(v) => try!(v.decode(), "width"),
        None => 0.0
    };

    let color = match fields.get("color") {
        Some(v) => try!(tracer::decode_parametric_number(v), "color"),
        None => return Err("missing field 'color'".into())
    };

    Ok(world::Object::Lamp(Lamp::Directional {
        direction: direction.normalize(),
        width: (width.to_radians() / 2.0).cos(),
        color: color,
    }))
}

fn decode_point(entry: Entry) -> Result<world::Object, String> {
    let fields = try!(entry.as_object().ok_or("not an object".into()));

    let position = match fields.get("position") {
        Some(v) => try!(v.dynamic_decode(), "position"),
        None => return Err("missing field 'position'".into())
    };

    let color = match fields.get("color") {
        Some(v) => try!(tracer::decode_parametric_number(v), "color"),
        None => return Err("missing field 'color'".into())
    };

    Ok(world::Object::Lamp(Lamp::Point(position, color)))
}
