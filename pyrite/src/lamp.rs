use std;
use std::sync::Arc;

use cgmath::{InnerSpace, Point3, Vector3};
use collision::Ray3;

use rand::Rng;

use crate::config::entry::Entry;
use crate::config::Prelude;

use crate::math::{
    utils::{sample_cone, sample_hemisphere, sample_sphere},
    RenderMath,
};
use crate::shapes::Shape;
use crate::{
    color::{decode_color, Color},
    materials::Material,
    world,
};

pub enum Lamp {
    Directional {
        direction: Vector3<f32>,
        width: f32,
        color: RenderMath<Color>,
    },
    Point(Point3<f32>, RenderMath<Color>),
    Shape(Arc<Shape>),
}

impl Lamp {
    pub fn sample(&self, rng: &mut impl Rng, target: Point3<f32>) -> Sample<'_> {
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
                    surface: Surface::Color(&color),
                    weight: 1.0,
                }
            }
            Lamp::Point(ref center, ref color) => {
                let v = center - target;
                let distance = v.magnitude2();
                Sample {
                    direction: v.normalize(),
                    sq_distance: Some(distance),
                    surface: Surface::Color(&color),
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

    pub fn sample_ray(&self, rng: &mut impl Rng) -> Option<RaySample<'_>> {
        match *self {
            Lamp::Directional { .. } => None,
            Lamp::Point(center, ref color) => {
                let direction = sample_sphere(rng);
                Some(RaySample {
                    ray: Ray3::new(center, direction),
                    surface: Surface::Color(&color),
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

pub struct Sample<'a> {
    pub direction: Vector3<f32>,
    pub sq_distance: Option<f32>,
    pub surface: Surface<'a>,
    pub weight: f32,
}

pub enum Surface<'a> {
    Physical {
        normal: Ray3<f32>,
        material: &'a Material,
    },
    Color(&'a RenderMath<Color>),
}

pub struct RaySample<'a> {
    pub ray: Ray3<f32>,
    pub surface: Surface<'a>,
    pub weight: f32,
}

pub fn register_types(context: &mut Prelude) {
    let mut group = context.object("Light".into());
    group
        .object("Directional".into())
        .add_decoder(decode_directional);
    group.object("Point".into()).add_decoder(decode_point);
}

fn decode_directional(entry: Entry<'_>) -> Result<world::Object, String> {
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
        Some(v) => try_for!(decode_color(v), "color"),
        None => return Err("missing field 'color'".into()),
    };

    Ok(world::Object::Lamp(Lamp::Directional {
        direction: direction.normalize(),
        width: (width.to_radians() / 2.0).cos(),
        color,
    }))
}

fn decode_point(entry: Entry<'_>) -> Result<world::Object, String> {
    let fields = entry.as_object().ok_or("not an object")?;

    let position = match fields.get("position") {
        Some(v) => try_for!(v.dynamic_decode(), "position"),
        None => return Err("missing field 'position'".into()),
    };

    let color = match fields.get("color") {
        Some(v) => try_for!(decode_color(v), "color"),
        None => return Err("missing field 'color'".into()),
    };

    Ok(world::Object::Lamp(Lamp::Point(position, color)))
}
