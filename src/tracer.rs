use std;

use rand::Rng;

use cgmath::{Vector, EuclideanVector, Vector3};
use cgmath::{Ray, Ray3};
use cgmath::{Point, Point3};

use config::{Value, Decode};
use config::entry::Entry;
use lamp::{self, Lamp};
use world::World;

pub use self::Reflection::{Reflect, Emit};

pub type Brdf = fn(ray_in: &Vector3<f64>, ray_out: &Vector3<f64>, normal: &Vector3<f64>) -> f64;
pub type Color = ParametricValue<RenderContext, f64>;

pub trait Material {
    fn reflect<'a>(&'a self, light: &mut Light, ray_in: &Ray3<f64>, normal: &Ray3<f64>, rng: &mut FloatRng) -> Reflection<'a>;
    fn get_emission<'a>(&'a self, light: &mut Light, ray_in: &Vector3<f64>, normal: &Ray3<f64>, rng: &mut FloatRng) -> Option<&'a Color>;
}

pub trait ParametricValue<From, To>: Send + Sync {
    fn get(&self, i: &From) -> To; 
}

impl<From> ParametricValue<From, f64> for f64 {
    fn get(&self, _: &From) -> f64 {
        *self
    }
}

pub trait FloatRng {
    fn next_float(&mut self) -> f64;
}

impl<R: Rng> FloatRng for R {
    fn next_float(&mut self) -> f64 {
        self.gen()
    }
}

pub enum Reflection<'a> {
    Emit(&'a Color),
    Reflect(Ray3<f64>, &'a Color, f64, Option<Brdf>)
}

pub struct RenderContext {
    pub wavelength: f64,
    pub normal: Vector3<f64>,
    pub incident: Vector3<f64>
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
    pub fn brdf(&self, incident: &Vector3<f64>, normal: &Vector3<f64>) -> f64 {
        if let BounceType::Diffuse(brdf, ref out) = *self {
            brdf(incident, normal, out)
        } else {
            1.0
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
    white: bool
}

impl Light {
    pub fn new(wavelength: f64) -> Light {
        Light {
            wavelength: wavelength,
            white: true
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

pub fn trace<'w, R: Rng + FloatRng>(rng: &mut R, mut ray: Ray3<f64>, mut light: Light, world: &'w World, bounces: u32, light_samples: usize) -> Vec<Bounce<'w>> {
    let mut sample_light = true;
    let mut path = Vec::with_capacity(bounces as usize);

    for _ in 0..bounces {
        match world.intersect(&ray) {
            Some((normal, material)) => match material.reflect(&mut light, &ray, &normal, &mut *rng as &mut FloatRng) {
                Reflect(out_ray, color, prob, brdf) => {

                    let direct_light = if let Some(brdf) = brdf {
                        trace_direct(rng, light_samples, light.clone(), &ray.direction, &normal, world, brdf)
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
                },
                Emit(color) => {
                    if sample_light {
                        path.push(Bounce {
                            ty: BounceType::Emission,
                            light: light,
                            color: color,
                            incident: ray.direction,
                            normal: normal,
                            probability: 1.0,
                            direct_light: vec![]
                        });
                    }

                    break;
                }
            },
            None => {
                let directional = if sample_light {
                    trace_directional(&ray.direction, world)
                } else {
                    None
                };
                let color = directional.unwrap_or_else(|| world.sky.color(&ray.direction));
                path.push(Bounce {
                    ty: BounceType::Emission,
                    light: light,
                    color: color,
                    incident: ray.direction,
                    normal: Ray3::new(Point3::from_vec(&ray.direction.mul_s(std::f64::INFINITY)), -ray.direction),
                    probability: 1.0,
                    direct_light: vec![]
                });

                break;
            }
        };
    }

    path
}

fn trace_direct<'w, R: Rng + FloatRng>(rng: &mut R, samples: usize, light: Light, ray_in: &Vector3<f64>, normal: &Ray3<f64>, world: &'w World, brdf: Brdf) -> Vec<DirectLight<'w>> {
    if let Some((lamp, probability)) = world.pick_lamp(rng) {
        let n = if ray_in.dot(&normal.direction) < 0.0 {
            normal.direction
        } else {
            -normal.direction
        };

        let normal = Ray::new(normal.origin, n);

        let weight = lamp.surface_area() / (samples as f64 * 2.0 * std::f64::consts::PI * probability);

        (0..samples).filter_map(|_| {
            let lamp::Sample {
                direction,
                sq_distance,
                surface
            } = lamp.sample(rng, normal.origin);
            
            let mut light = light.clone();

            let ray_out = Ray::new(normal.origin, direction);

            let cos_out = normal.direction.dot(&ray_out.direction).max(0.0);

            if cos_out > 0.0 {
                let hit_dist = world.intersect(&ray_out).map(|(hit_normal, _)| hit_normal.origin.sub_p(&normal.origin).length2());

                let blocked = match (hit_dist, sq_distance) {
                    (Some(hit), Some(lamp)) if hit >= lamp - 0.0000001 => false,
                    (None, _) => false,
                    _ => true,
                };

                if !blocked {
                    let (color, cos_in, target_normal) = match surface {
                        lamp::Surface::Physical {
                            normal: target_normal,
                            material
                        } => {
                            let color = material.get_emission(&mut light, &ray_out.direction, &target_normal, &mut *rng as &mut FloatRng);
                            let cos_in = target_normal.direction.dot(& -ray_out.direction).abs();
                            (color, cos_in, target_normal.direction)
                        },
                        lamp::Surface::Color(color) => {
                            let target_normal = -ray_out.direction;
                            (Some(color), 1.0, target_normal)
                        },
                    };
                    let scale = weight * cos_in * brdf(ray_in, &normal.direction, &ray_out.direction) / sq_distance.unwrap_or(1.0);
                    
                    return color.map(|color| {
                        DirectLight {
                            light: light,
                            color: color,
                            incident: ray_out.direction,
                            normal: target_normal,
                            probability: scale
                        }
                    });
                }
            }

            None
        }).collect()
    } else {
        vec![]
    }
}


fn trace_directional<'w>(ray: &Vector3<f64>, world: &'w World) -> Option<&'w Color> {
    for light in &world.lights {
        if let &Lamp::Directional { direction, width, ref color } = light {
            if direction.dot(ray) >= width {
                return Some(&**color);
            }
        }
    }
    
    None
}

pub fn decode_parametric_number<From: Decode + 'static>(item: Entry) -> Result<Box<ParametricValue<From, f64>>, String> {
    if let Some(&Value::Number(num)) = item.as_value() {
        Ok(Box::new(num.as_float()) as Box<ParametricValue<From, f64>>)
    } else {
        item.dynamic_decode()
    }
}