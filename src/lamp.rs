use std;
use std::sync::Arc;

use cgmath::{EuclideanVector, Vector3, Point, Point3, Ray3};

use rand::Rng;

use config::Prelude;
use config::entry::Entry;

use tracer::{self, FloatRng, Material, ParametricValue, Color};
use shapes::Shape;
use world;
use math::utils::sample_cone;

pub enum Lamp {
    Directional {
        direction: Vector3<f64>,
        width: f64,
        color: Box<Color>
    },
    Point(Point3<f64>, Box<Color>),
    Shape(Arc<Shape>)
}

impl Lamp {
    pub fn sample<R: Rng + FloatRng>(&self, rng: &mut R, target: Point3<f64>) -> Sample {
        match *self {
            Lamp::Directional { direction, width, ref color } => {
                let dir = if width > 0.0 {
                    sample_cone(rng, &direction, width)
                } else {
                    direction
                };

                Sample {
                    direction: dir,
                    sq_distance: None,
                    surface: Surface::Color(&**color),
                    weight: 1.0
                }
            },
            Lamp::Point(ref center, ref color) => {
                let v = center.sub_p(&target);
                let distance = v.length2();
                Sample {
                    direction: v.normalize(),
                    sq_distance: Some(distance),
                    surface: Surface::Color(&**color),
                    weight: 4.0 * std::f64::consts::PI / distance
                }
            },
            Lamp::Shape(ref shape) => {
                let ray = shape.sample_towards(rng, &target).expect("trying to use infinite shape in direct lighting");
                let v = ray.origin.sub_p(&target);
                let distance = v.length2();
                let weight = shape.area_towards(&target).unwrap_or_else(|| {
                    shape.surface_area() / distance
                });
                Sample {
                    direction: v.normalize(),
                    sq_distance: Some(distance),
                    surface: Surface::Physical {
                        normal: ray,
                        material: shape.get_material()
                    },
                    weight: weight
                }
            }
        }
    }
}

pub struct Sample<'a> {
    pub direction: Vector3<f64>,
    pub sq_distance: Option<f64>,
    pub surface: Surface<'a>,
    pub weight: f64,
}

pub enum Surface<'a> {
    Physical {
        normal: Ray3<f64>,
        material: &'a Material
    },
    Color(&'a Color),
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