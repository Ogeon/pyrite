use std;
use std::sync::Arc;

use cgmath::{InnerSpace, Point3, Vector3};
use collision::Ray3;

use rand::Rng;

use crate::config::entry::Entry;
use crate::config::Prelude;

use crate::math::utils::{sample_cone, sample_hemisphere, sample_sphere};
use crate::shapes::Shape;
use crate::tracer::{self, Color, Material};
use crate::world;

pub enum Lamp<R: Rng> {
    Directional {
        direction: Vector3<f32>,
        width: f32,
        color: Box<Color>,
    },
    Point(Point3<f32>, Box<Color>),
    Shape(Arc<Shape<R>>),
}

impl<R: Rng> Lamp<R> {
    pub fn sample(&self, rng: &mut R, target: Point3<f32>) -> Sample<'_, R> {
        match *self {
            Lamp::Directional {
                direction,
                width,
                ref color,
            } => {
                let dir = if width > 0.0 {
                    sample_cone(rng, direction, width)
                } else {
                    direction
                };

                Sample {
                    direction: dir,
                    sq_distance: None,
                    surface: Surface::Color(&**color),
                    weight: 1.0,
                }
            }
            Lamp::Point(ref center, ref color) => {
                let v = center - target;
                let distance = v.magnitude2();
                Sample {
                    direction: v.normalize(),
                    sq_distance: Some(distance),
                    surface: Surface::Color(&**color),
                    weight: 4.0 * std::f32::consts::PI / distance,
                }
            }
            Lamp::Shape(ref shape) => {
                let ray = shape
                    .sample_towards(rng, &target)
                    .expect("trying to use infinite shape in direct lighting");
                let v = ray.origin - target;
                let distance = v.magnitude2();
                let direction = v.normalize();
                let weight = shape.solid_angle_towards(&target).unwrap_or_else(|| {
                    let cos_in = ray.direction.dot(-direction).abs();
                    cos_in * shape.surface_area() / distance
                });
                Sample {
                    direction: direction,
                    sq_distance: Some(distance),
                    surface: Surface::Physical {
                        normal: ray,
                        material: shape.get_material(),
                    },
                    weight: weight,
                }
            }
        }
    }

    pub fn sample_ray(&self, rng: &mut R) -> Option<RaySample<'_, R>> {
        match *self {
            Lamp::Directional { .. } => None,
            Lamp::Point(center, ref color) => {
                let direction = sample_sphere(rng);
                Some(RaySample {
                    ray: Ray3::new(center, direction),
                    surface: Surface::Color(&**color),
                    weight: (4.0 * std::f32::consts::PI),
                })
            }
            Lamp::Shape(ref shape) => {
                let normal = shape
                    .sample_point(rng)
                    .expect("trying to use infinite shape as lamp");
                let direction = sample_hemisphere(rng, normal.direction);
                Some(RaySample {
                    ray: Ray3::new(normal.origin, direction),
                    surface: Surface::Physical {
                        normal: normal,
                        material: shape.get_material(),
                    },
                    weight: shape.surface_area(),
                })
            }
        }
    }
}

pub struct Sample<'a, R: Rng> {
    pub direction: Vector3<f32>,
    pub sq_distance: Option<f32>,
    pub surface: Surface<'a, R>,
    pub weight: f32,
}

pub enum Surface<'a, R: Rng> {
    Physical {
        normal: Ray3<f32>,
        material: &'a dyn Material<R>,
    },
    Color(&'a Color),
}

pub struct RaySample<'a, R: Rng> {
    pub ray: Ray3<f32>,
    pub surface: Surface<'a, R>,
    pub weight: f32,
}

pub fn register_types<R: Rng + 'static>(context: &mut Prelude) {
    let mut group = context.object("Light".into());
    group
        .object("Directional".into())
        .add_decoder(decode_directional::<R>);
    group.object("Point".into()).add_decoder(decode_point::<R>);
}

fn decode_directional<R: Rng>(entry: Entry<'_>) -> Result<world::Object<R>, String> {
    let fields = entry.as_object().ok_or("not an object")?;

    let direction: Vector3<_> = match fields.get("direction") {
        Some(v) => try_for!(v.dynamic_decode(), "direction"),
        None => return Err("missing field 'direction'".into()),
    };

    let width: f32 = match fields.get("width") {
        Some(v) => try_for!(v.decode(), "width"),
        None => 0.0,
    };

    let color = match fields.get("color") {
        Some(v) => try_for!(tracer::decode_parametric_number(v), "color"),
        None => return Err("missing field 'color'".into()),
    };

    Ok(world::Object::Lamp(Lamp::Directional {
        direction: direction.normalize(),
        width: (width.to_radians() / 2.0).cos(),
        color: color,
    }))
}

fn decode_point<R: Rng>(entry: Entry<'_>) -> Result<world::Object<R>, String> {
    let fields = entry.as_object().ok_or("not an object")?;

    let position = match fields.get("position") {
        Some(v) => try_for!(v.dynamic_decode(), "position"),
        None => return Err("missing field 'position'".into()),
    };

    let color = match fields.get("color") {
        Some(v) => try_for!(tracer::decode_parametric_number(v), "color"),
        None => return Err("missing field 'color'".into()),
    };

    Ok(world::Object::Lamp(Lamp::Point(position, color)))
}
