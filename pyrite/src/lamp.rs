use std;

use cgmath::{InnerSpace, Point2, Point3, Vector3};
use collision::Ray3;

use rand::Rng;

use crate::math::utils::{sample_cone, sample_hemisphere, sample_sphere};
use crate::shapes::{Intersection, Shape, SurfaceData};
use crate::{materials::Material, tracer::LightProgram};

pub(crate) enum Lamp<'p> {
    Directional {
        direction: Vector3<f32>,
        width: f32,
        color: LightProgram<'p>,
    },
    Point(Point3<f32>, LightProgram<'p>),
    Shape(&'p Shape<'p>),
}

impl<'p> Lamp<'p> {
    pub fn sample(&self, rng: &mut impl Rng, target: Point3<f32>) -> Sample<'_> {
        match *self {
            Lamp::Directional {
                direction,
                width,
                color,
            } => {
                let dir = if width > 0.0 {
                    sample_cone(rng, direction, width)
                } else {
                    direction
                };

                Sample {
                    direction: dir,
                    sq_distance: None,
                    surface: Surface::Color(color),
                    weight: 1.0,
                }
            }
            Lamp::Point(ref center, color) => {
                let v = center - target;
                let distance = v.magnitude2();
                Sample {
                    direction: v.normalize(),
                    sq_distance: Some(distance),
                    surface: Surface::Color(color),
                    weight: 4.0 * std::f32::consts::PI / distance,
                }
            }
            Lamp::Shape(ref shape) => {
                let Intersection {
                    distance,
                    surface_point,
                } = shape
                    .sample_towards(rng, &target)
                    .expect("trying to use infinite shape in direct lighting");
                let v = surface_point.position - target;
                let sq_distance = distance * distance;
                let direction = v.normalize();

                let SurfaceData { normal, texture } = surface_point.get_surface_data();

                let weight = shape.solid_angle_towards(&target).unwrap_or_else(|| {
                    let cos_in = normal.vector().dot(-direction).abs();
                    cos_in * shape.surface_area() / sq_distance
                });
                Sample {
                    direction,
                    sq_distance: Some(sq_distance),
                    surface: Surface::Physical {
                        normal: normal.vector(),
                        texture,
                        material: shape.get_material(),
                    },
                    weight,
                }
            }
        }
    }

    pub fn sample_ray(&self, rng: &mut impl Rng) -> Option<RaySample<'_>> {
        match *self {
            Lamp::Directional { .. } => None,
            Lamp::Point(center, color) => {
                let direction = sample_sphere(rng);
                Some(RaySample {
                    ray: Ray3::new(center, direction),
                    surface: Surface::Color(color),
                    weight: (4.0 * std::f32::consts::PI),
                })
            }
            Lamp::Shape(ref shape) => {
                let surface_point = shape
                    .sample_point(rng)
                    .expect("trying to use infinite shape as lamp");

                let SurfaceData { normal, texture } = surface_point.get_surface_data();
                let direction = sample_hemisphere(rng, normal.vector());
                Some(RaySample {
                    ray: Ray3::new(surface_point.position, direction),
                    surface: Surface::Physical {
                        normal: normal.vector(),
                        texture,
                        material: shape.get_material(),
                    },
                    weight: shape.surface_area(),
                })
            }
        }
    }
}

pub(crate) struct Sample<'a> {
    pub direction: Vector3<f32>,
    pub sq_distance: Option<f32>,
    pub surface: Surface<'a>,
    pub weight: f32,
}

pub(crate) enum Surface<'a> {
    Physical {
        normal: Vector3<f32>,
        texture: Point2<f32>,
        material: &'a Material<'a>,
    },
    Color(LightProgram<'a>),
}

pub(crate) struct RaySample<'a> {
    pub ray: Ray3<f32>,
    pub surface: Surface<'a>,
    pub weight: f32,
}
