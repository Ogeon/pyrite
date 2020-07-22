use std::{self, error::Error};

use rand::Rng;

use cgmath::{EuclideanSpace, InnerSpace, Point2, Point3, Vector3};
use collision::Ray3;

use crate::{
    color,
    lamp::{self, Lamp},
    math::DIST_EPSILON,
    project::program::{ExecutionContext, InputFn, Program, ProgramInput},
    world::World,
};

pub(crate) use self::Reflection::{Emit, Reflect};
use color::WavelengthInput;

pub type Brdf = fn(ray_in: Vector3<f32>, ray_out: Vector3<f32>, normal: Vector3<f32>) -> f32;
pub(crate) type LightProgram<'p> = Program<'p, RenderContext, color::Light>;

pub trait ParametricValue<From, To>: Send + Sync {
    fn get(&self, i: &From) -> To;
}

impl<From> ParametricValue<From, f32> for f32 {
    fn get(&self, _: &From) -> f32 {
        *self
    }
}

pub(crate) enum Reflection<'a> {
    Emit(LightProgram<'a>),
    Reflect(Ray3<f32>, LightProgram<'a>, f32, Option<Brdf>),
}

pub struct NormalInput {
    pub normal: Vector3<f32>,
    pub incident: Vector3<f32>,
    pub texture: Point2<f32>,
}

impl ProgramInput for NormalInput {
    fn normal() -> Result<InputFn<Self>, Box<dyn Error>> {
        Ok(|_, this, _| this.normal.into())
    }
    fn incident() -> Result<InputFn<Self>, Box<dyn Error>> {
        Ok(|_, this, _| this.incident.into())
    }
    fn texture_coordinates() -> Result<InputFn<Self>, Box<dyn Error>> {
        Ok(|_, this, _| this.texture.into())
    }
}

pub struct RenderContext {
    pub wavelength: f32,
    pub normal: Vector3<f32>,
    pub incident: Vector3<f32>,
    pub texture: Point2<f32>,
}

impl ProgramInput for RenderContext {
    fn normal() -> Result<InputFn<Self>, Box<dyn Error>> {
        Ok(|_, this, _| this.normal.into())
    }
    fn incident() -> Result<InputFn<Self>, Box<dyn Error>> {
        Ok(|_, this, _| this.incident.into())
    }
    fn texture_coordinates() -> Result<InputFn<Self>, Box<dyn Error>> {
        Ok(|_, this, _| this.texture.into())
    }
}

impl WavelengthInput for RenderContext {
    fn wavelength(&self) -> f32 {
        self.wavelength
    }
}

pub(crate) struct Bounce<'a> {
    pub ty: BounceType,
    pub light: Light,
    pub color: LightProgram<'a>,
    pub incident: Vector3<f32>,
    pub position: Point3<f32>,
    pub normal: Vector3<f32>,
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

pub(crate) struct DirectLight<'a> {
    pub light: Light,
    pub color: LightProgram<'a>,
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

pub(crate) fn trace<'w, R: Rng>(
    path: &mut Vec<Bounce<'w>>,
    rng: &mut R,
    mut ray: Ray3<f32>,
    mut light: Light,
    world: &'w World,
    bounces: u32,
    light_samples: usize,
    exe: &mut ExecutionContext<'w>,
) {
    let mut sample_light = true;

    for _ in 0..bounces {
        match world.intersect(ray) {
            Some(intersection) => {
                let material = intersection.surface_point.get_material();
                let surface_data = intersection.surface_point.get_surface_data();

                let normal_input = NormalInput {
                    incident: ray.direction,
                    normal: surface_data.normal.vector(),
                    texture: surface_data.texture,
                };
                let normal = material.apply_normal_map(surface_data.normal, normal_input, exe);
                let position = intersection.surface_point.position;

                match material.reflect(&mut light, ray, position, normal, rng) {
                    Reflect(out_ray, color, prob, brdf) => {
                        let direct_light = if let Some(brdf) = brdf {
                            trace_direct(
                                rng,
                                light_samples,
                                light.clone(),
                                ray.direction,
                                position,
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
                            position,
                            normal,
                            texture: surface_data.texture,
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
                                position,
                                normal,
                                texture: surface_data.texture,
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
                let color = directional.unwrap_or_else(|| world.sky);
                path.push(Bounce {
                    ty: BounceType::Emission,
                    light,
                    color,
                    incident: ray.direction,
                    position: Point3::from_vec(&ray.direction * std::f32::INFINITY),
                    normal: -ray.direction,
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
    position: Point3<f32>,
    normal: Vector3<f32>,
    world: &'w World,
    brdf: Brdf,
) -> Vec<DirectLight<'w>> {
    if let Some((lamp, probability)) = world.pick_lamp(rng) {
        let normal = if ray_in.dot(normal) < 0.0 {
            normal
        } else {
            -normal
        };

        let probability = 1.0 / (samples as f32 * 2.0 * std::f32::consts::PI * probability);

        (0..samples)
            .filter_map(|_| {
                let lamp::Sample {
                    direction,
                    sq_distance,
                    surface,
                    weight,
                } = lamp.sample(rng, position);

                let mut light = light.clone();

                let ray_out = Ray3::new(position, direction);

                let cos_out = normal.dot(ray_out.direction).max(0.0);

                if cos_out > 0.0 {
                    let hit_dist = world
                        .intersect(ray_out)
                        .map(|hit| hit.distance * hit.distance);

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
                                (color, target_normal)
                            }
                            lamp::Surface::Color(color) => {
                                let target_normal = -ray_out.direction;
                                (Some(color), target_normal)
                            }
                        };
                        let scale = weight * probability * brdf(ray_in, normal, ray_out.direction);

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

fn trace_directional<'w>(ray: Vector3<f32>, world: &'w World) -> Option<LightProgram<'w>> {
    for light in &world.lights {
        if let &Lamp::Directional {
            direction,
            width,
            color,
        } = light
        {
            if direction.dot(ray) >= width {
                return Some(color);
            }
        }
    }

    None
}
