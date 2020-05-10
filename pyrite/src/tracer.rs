use std;

use rand::Rng;

use cgmath::{EuclideanSpace, InnerSpace, Point3, Vector3};
use collision::Ray3;

use config::entry::Entry;
use config::{Decode, Value};
use lamp::{self, Lamp};
use world::World;

pub use self::Reflection::{Emit, Reflect};

pub type Brdf = fn(ray_in: Vector3<f64>, ray_out: Vector3<f64>, normal: Vector3<f64>) -> f64;
pub type Color = dyn ParametricValue<RenderContext, f64>;

pub trait Material<R: Rng>: Sync {
    fn reflect<'a>(
        &'a self,
        light: &mut Light,
        ray_in: Ray3<f64>,
        normal: Ray3<f64>,
        rng: &mut R,
    ) -> Reflection<'a>;
    fn get_emission<'a>(
        &'a self,
        light: &mut Light,
        ray_in: Vector3<f64>,
        normal: Ray3<f64>,
        rng: &mut R,
    ) -> Option<&'a Color>;
}

pub trait ParametricValue<From, To>: Send + Sync {
    fn get(&self, i: &From) -> To;
}

impl<From> ParametricValue<From, f64> for f64 {
    fn get(&self, _: &From) -> f64 {
        *self
    }
}

pub enum Reflection<'a> {
    Emit(&'a Color),
    Reflect(Ray3<f64>, &'a Color, f64, Option<Brdf>),
}

pub struct RenderContext {
    pub wavelength: f64,
    pub normal: Vector3<f64>,
    pub incident: Vector3<f64>,
}

pub struct Bounce<'a> {
    pub ty: BounceType,
    pub light: Light,
    pub color: &'a Color,
    pub incident: Vector3<f64>,
    pub normal: Ray3<f64>,
    pub probability: f64,
    pub direct_light: Vec<DirectLight<'a>>,
}

pub enum BounceType {
    Diffuse(Brdf, Vector3<f64>),
    Specular,
    Emission,
}

impl BounceType {
    pub fn brdf(&self, incident: Vector3<f64>, normal: Vector3<f64>) -> f64 {
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
    pub color: &'a Color,
    pub incident: Vector3<f64>,
    pub normal: Vector3<f64>,
    pub probability: f64,
}

#[derive(Clone)]
pub struct Light {
    wavelength: f64,
    white: bool,
}

impl Light {
    pub fn new(wavelength: f64) -> Light {
        Light {
            wavelength: wavelength,
            white: true,
        }
    }

    pub fn colored(&mut self) -> f64 {
        self.white = false;
        self.wavelength
    }

    pub fn is_white(&self) -> bool {
        self.white
    }
}

pub fn trace<'w, R: Rng>(
    rng: &mut R,
    mut ray: Ray3<f64>,
    mut light: Light,
    world: &'w World<R>,
    bounces: u32,
    light_samples: usize,
) -> Vec<Bounce<'w>> {
    let mut sample_light = true;
    let mut path = Vec::with_capacity(bounces as usize);

    for _ in 0..bounces {
        match world.intersect(&ray) {
            Some((normal, material)) => match material.reflect(&mut light, ray, normal, rng) {
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
                        color: color,
                        incident: ray.direction,
                        normal: normal,
                        probability: prob,
                        direct_light: direct_light,
                    };

                    ray = out_ray;
                    path.push(bounce);
                }
                Emit(color) => {
                    if sample_light {
                        path.push(Bounce {
                            ty: BounceType::Emission,
                            light: light,
                            color: color,
                            incident: ray.direction,
                            normal: normal,
                            probability: 1.0,
                            direct_light: vec![],
                        });
                    }

                    break;
                }
            },
            None => {
                let directional = if sample_light {
                    trace_directional(ray.direction, world)
                } else {
                    None
                };
                let color = directional.unwrap_or_else(|| world.sky.color(&ray.direction));
                path.push(Bounce {
                    ty: BounceType::Emission,
                    light: light,
                    color: color,
                    incident: ray.direction,
                    normal: Ray3::new(
                        Point3::from_vec(&ray.direction * std::f64::INFINITY),
                        -ray.direction,
                    ),
                    probability: 1.0,
                    direct_light: vec![],
                });

                break;
            }
        };
    }

    path
}

fn trace_direct<'w, R: Rng>(
    rng: &mut R,
    samples: usize,
    light: Light,
    ray_in: Vector3<f64>,
    normal: Ray3<f64>,
    world: &'w World<R>,
    brdf: Brdf,
) -> Vec<DirectLight<'w>> {
    if let Some((lamp, probability)) = world.pick_lamp(rng) {
        let n = if ray_in.dot(normal.direction) < 0.0 {
            normal.direction
        } else {
            -normal.direction
        };

        let normal = Ray3::new(normal.origin, n);

        let probability = 1.0 / (samples as f64 * 2.0 * std::f64::consts::PI * probability);

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
                        .map(|(hit_normal, _)| (hit_normal.origin - &normal.origin).magnitude2());

                    let blocked = match (hit_dist, sq_distance) {
                        (Some(hit), Some(lamp)) if hit >= lamp - 0.0000001 => false,
                        (None, _) => false,
                        _ => true,
                    };

                    if !blocked {
                        let (color, target_normal) = match surface {
                            lamp::Surface::Physical {
                                normal: target_normal,
                                material,
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
                            light: light,
                            color: color,
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

fn trace_directional<'w, R: Rng>(ray: Vector3<f64>, world: &'w World<R>) -> Option<&'w Color> {
    for light in &world.lights {
        if let &Lamp::Directional {
            direction,
            width,
            ref color,
        } = light
        {
            if direction.dot(ray) >= width {
                return Some(&**color);
            }
        }
    }

    None
}

pub fn decode_parametric_number<From: Decode + 'static>(
    item: Entry,
) -> Result<Box<dyn ParametricValue<From, f64>>, String> {
    if let Some(&Value::Number(num)) = item.as_value() {
        Ok(Box::new(num.as_float()) as Box<dyn ParametricValue<From, f64>>)
    } else {
        item.dynamic_decode()
    }
}
