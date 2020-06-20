use std;

use rand::Rng;

use cgmath::{EuclideanSpace, InnerSpace, Point2, Point3, Vector3};
use collision::Ray3;

use crate::{
    color::Color,
    lamp::{self, Lamp},
    math::{RenderMath, DIST_EPSILON},
    world::World,
};

pub use self::Reflection::{Emit, Reflect};

pub type Brdf = fn(ray_in: Vector3<f32>, ray_out: Vector3<f32>, normal: Vector3<f32>) -> f32;

pub trait ParametricValue<From, To>: Send + Sync {
    fn get(&self, i: &From) -> To;
}

impl<From> ParametricValue<From, f32> for f32 {
    fn get(&self, _: &From) -> f32 {
        *self
    }
}

pub enum Reflection<'a> {
    Emit(&'a RenderMath<Color>),
    Reflect(Ray3<f32>, &'a RenderMath<Color>, f32, Option<Brdf>),
}

pub struct RenderContext {
    pub wavelength: f32,
    pub normal: Vector3<f32>,
    pub incident: Vector3<f32>,
    pub texture: Point2<f32>,
}

pub struct Bounce<'a> {
    pub ty: BounceType,
    pub light: Light,
    pub color: &'a RenderMath<Color>,
    pub incident: Vector3<f32>,
    pub normal: Ray3<f32>,
    pub texture: Point2<f32>,
    pub probability: f32,
    pub direct_light: Vec<DirectLight<'a>>,
}

pub enum BounceType {
    Diffuse(Brdf, Vector3<f32>),
    Specular,
    Emission,
}

impl BounceType {
    pub fn brdf(&self, incident: Vector3<f32>, normal: Vector3<f32>) -> f32 {
        if let BounceType::Diffuse(brdf, out) = *self {
            brdf(incident, normal, out)
        } else {
            1.0
        }
    }

    pub fn is_emission(&self) -> bool {
        if let BounceType::Emission = *self {
            true
        } else {
            false
        }
    }
}

pub struct DirectLight<'a> {
    pub light: Light,
    pub color: &'a RenderMath<Color>,
    pub incident: Vector3<f32>,
    pub normal: Vector3<f32>,
    pub probability: f32,
}

#[derive(Clone)]
pub struct Light {
    wavelength: f32,
    white: bool,
}

impl Light {
    pub fn new(wavelength: f32) -> Light {
        Light {
            wavelength: wavelength,
            white: true,
        }
    }

    pub fn colored(&mut self) -> f32 {
        self.white = false;
        self.wavelength
    }

    pub fn is_white(&self) -> bool {
        self.white
    }
}

pub fn trace<'w, R: Rng>(
    path: &mut Vec<Bounce<'w>>,
    rng: &mut R,
    mut ray: Ray3<f32>,
    mut light: Light,
    world: &'w World,
    bounces: u32,
    light_samples: usize,
) {
    let mut sample_light = true;

    for _ in 0..bounces {
        match world.intersect(&ray) {
            Some((intersection, material)) => {
                let normal = Ray3::new(intersection.position, intersection.normal);

                match material.reflect(&mut light, ray, normal, rng) {
                    Reflect(out_ray, color, prob, brdf) => {
                        let direct_light = if let Some(brdf) = brdf {
                            trace_direct(
                                rng,
                                light_samples,
                                light.clone(),
                                ray.direction,
                                normal,
                                world,
                                brdf,
                            )
                        } else {
                            vec![]
                        };

                        sample_light = brdf.is_none() || light_samples == 0;

                        let bounce_type = if let Some(brdf) = brdf {
                            BounceType::Diffuse(brdf, out_ray.direction)
                        } else {
                            BounceType::Specular
                        };

                        let bounce = Bounce {
                            ty: bounce_type,
                            light: light.clone(),
                            color,
                            incident: ray.direction,
                            normal,
                            texture: intersection.texture,
                            probability: prob,
                            direct_light,
                        };

                        ray = out_ray;
                        path.push(bounce);
                    }
                    Emit(color) => {
                        if sample_light {
                            path.push(Bounce {
                                ty: BounceType::Emission,
                                light,
                                color,
                                incident: ray.direction,
                                normal,
                                texture: intersection.texture,
                                probability: 1.0,
                                direct_light: vec![],
                            });
                        }

                        break;
                    }
                }
            }
            None => {
                let directional = if sample_light {
                    trace_directional(ray.direction, world)
                } else {
                    None
                };
                let color = directional.unwrap_or_else(|| &world.sky);
                path.push(Bounce {
                    ty: BounceType::Emission,
                    light,
                    color,
                    incident: ray.direction,
                    normal: Ray3::new(
                        Point3::from_vec(&ray.direction * std::f32::INFINITY),
                        -ray.direction,
                    ),
                    texture: Point2::origin(),
                    probability: 1.0,
                    direct_light: vec![],
                });

                break;
            }
        };
    }
}

fn trace_direct<'w, R: Rng>(
    rng: &mut R,
    samples: usize,
    light: Light,
    ray_in: Vector3<f32>,
    normal: Ray3<f32>,
    world: &'w World,
    brdf: Brdf,
) -> Vec<DirectLight<'w>> {
    if let Some((lamp, probability)) = world.pick_lamp(rng) {
        let n = if ray_in.dot(normal.direction) < 0.0 {
            normal.direction
        } else {
            -normal.direction
        };

        let normal = Ray3::new(normal.origin, n);

        let probability = 1.0 / (samples as f32 * 2.0 * std::f32::consts::PI * probability);

        (0..samples)
            .filter_map(|_| {
                let lamp::Sample {
                    direction,
                    sq_distance,
                    surface,
                    weight,
                } = lamp.sample(rng, normal.origin);

                let mut light = light.clone();

                let ray_out = Ray3::new(normal.origin, direction);

                let cos_out = normal.direction.dot(ray_out.direction).max(0.0);

                if cos_out > 0.0 {
                    let hit_dist = world
                        .intersect(&ray_out)
                        .map(|(hit, _)| hit.distance * hit.distance);

                    let blocked = match (hit_dist, sq_distance) {
                        (Some(hit), Some(lamp)) if hit >= lamp - DIST_EPSILON => false,
                        (None, _) => false,
                        _ => true,
                    };

                    if !blocked {
                        let (color, target_normal) = match surface {
                            lamp::Surface::Physical {
                                normal: target_normal,
                                material,
                                ..
                            } => {
                                let color = material.get_emission(
                                    &mut light,
                                    ray_out.direction,
                                    target_normal,
                                    rng,
                                );
                                (color, target_normal.direction)
                            }
                            lamp::Surface::Color(color) => {
                                let target_normal = -ray_out.direction;
                                (Some(color), target_normal)
                            }
                        };
                        let scale = weight
                            * probability
                            * brdf(ray_in, normal.direction, ray_out.direction);

                        return color.map(|color| DirectLight {
                            light,
                            color,
                            incident: ray_out.direction,
                            normal: target_normal,
                            probability: scale,
                        });
                    }
                }

                None
            })
            .collect()
    } else {
        vec![]
    }
}

fn trace_directional<'w>(ray: Vector3<f32>, world: &'w World) -> Option<&'w RenderMath<Color>> {
    for light in &world.lights {
        if let &Lamp::Directional {
            direction,
            width,
            ref color,
        } = light
        {
            if direction.dot(ray) >= width {
                return Some(color);
            }
        }
    }

    None
}
