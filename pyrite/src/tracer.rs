use rand::Rng;

use cgmath::{EuclideanSpace, InnerSpace, Point2, Point3, Vector3};
use collision::Ray3;

use crate::{
    lamp::{self, Lamp},
    materials::{ProbabilityInput, Scattering},
    math::DIST_EPSILON,
    program::{ExecutionContext, NumberInput, ProgramFor, ProgramInput, VectorInput},
    project::expressions::Vector,
    world::World,
};

use std::{borrow::Cow, cell::Cell, convert::TryFrom};

pub type Brdf = fn(ray_in: Vector3<f32>, ray_out: Vector3<f32>, normal: Vector3<f32>) -> f32;
pub(crate) type LightProgram<'p> = ProgramFor<'p, RenderContext, f32>;

pub trait ParametricValue<From, To>: Send + Sync {
    fn get(&self, i: &From) -> To;
}

impl<From> ParametricValue<From, f32> for f32 {
    fn get(&self, _: &From) -> f32 {
        *self
    }
}

pub struct NormalInput {
    pub normal: Vector3<f32>,
    pub incident: Vector3<f32>,
    pub texture: Point2<f32>,
}

impl ProgramInput for NormalInput {
    type NumberInput = NormalNumberInput;
    type VectorInput = SurfaceVectorInput;

    #[inline(always)]
    fn get_number_input(&self, input: Self::NumberInput) -> f32 {
        match input {}
    }

    #[inline(always)]
    fn get_vector_input(&self, input: Self::VectorInput) -> Vector {
        match input {
            SurfaceVectorInput::Normal => self.normal.into(),
            SurfaceVectorInput::Incident => self.incident.into(),
            SurfaceVectorInput::TextureCoordinates => self.texture.into(),
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) enum NormalNumberInput {}

impl TryFrom<NumberInput> for NormalNumberInput {
    type Error = Cow<'static, str>;

    fn try_from(value: NumberInput) -> Result<Self, Self::Error> {
        match value {
            NumberInput::Wavelength => {
                Err("the wavelength is not available during normal mapping".into())
            }
        }
    }
}

pub struct RenderContext {
    pub wavelength: f32,
    pub normal: Vector3<f32>,
    pub incident: Vector3<f32>,
    pub texture: Point2<f32>,
}

impl ProgramInput for RenderContext {
    type NumberInput = RenderNumberInput;
    type VectorInput = SurfaceVectorInput;

    #[inline(always)]
    fn get_number_input(&self, input: Self::NumberInput) -> f32 {
        match input {
            RenderNumberInput::Wavelength => self.wavelength,
        }
    }

    #[inline(always)]
    fn get_vector_input(&self, input: Self::VectorInput) -> Vector {
        match input {
            SurfaceVectorInput::Normal => self.normal.into(),
            SurfaceVectorInput::Incident => self.incident.into(),
            SurfaceVectorInput::TextureCoordinates => self.texture.into(),
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) enum RenderNumberInput {
    Wavelength,
}

impl TryFrom<NumberInput> for RenderNumberInput {
    type Error = Cow<'static, str>;

    fn try_from(value: NumberInput) -> Result<Self, Self::Error> {
        match value {
            NumberInput::Wavelength => Ok(RenderNumberInput::Wavelength),
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) enum SurfaceVectorInput {
    Normal,
    Incident,
    TextureCoordinates,
}

impl TryFrom<VectorInput> for SurfaceVectorInput {
    type Error = Cow<'static, str>;

    fn try_from(value: VectorInput) -> Result<Self, Self::Error> {
        match value {
            VectorInput::Normal => Ok(SurfaceVectorInput::Normal),
            VectorInput::Incident => Ok(SurfaceVectorInput::Incident),
            VectorInput::TextureCoordinates => Ok(SurfaceVectorInput::TextureCoordinates),
        }
    }
}

pub(crate) struct Bounce<'a> {
    pub ty: BounceType,
    pub dispersed: bool,
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
    pub dispersed: bool,
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

pub(crate) fn trace<'w, R: Rng>(
    path: &mut Vec<Bounce<'w>>,
    rng: &mut R,
    mut ray: Ray3<f32>,
    wavelength: f32,
    world: &'w World,
    bounces: u32,
    light_samples: usize,
    exe: &mut ExecutionContext<'w>,
) {
    let mut sample_light = true;
    let mut light_sample_events = 0;

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

                let component = material.choose_component(rng);

                let probability_input = ProbabilityInput {
                    wavelength,
                    wavelength_used: Cell::new(false),
                    normal,
                    incident: ray.direction,
                    texture_coordinate: surface_data.texture,
                };
                let component_probability = component.get_probability(exe, &probability_input);
                let normal_dispersed = probability_input.wavelength_used.get();

                let scattered = component
                    .bsdf
                    .scatter(ray.direction, normal, wavelength, rng);
                match scattered {
                    Scattering::Reflected {
                        out_direction,
                        probability,
                        dispersed,
                        brdf,
                    } => {
                        let direct_light = if light_sample_events < 2 {
                            sample_light = brdf.is_none() || light_samples == 0;

                            if let Some(brdf) = brdf {
                                light_sample_events += 1;

                                trace_direct(
                                    rng,
                                    light_samples,
                                    wavelength,
                                    ray.direction,
                                    position,
                                    normal,
                                    world,
                                    brdf,
                                    exe,
                                )
                            } else {
                                vec![]
                            }
                        } else {
                            sample_light = true;
                            vec![]
                        };

                        let bounce_type = if let Some(brdf) = brdf {
                            BounceType::Diffuse(brdf, out_direction)
                        } else {
                            BounceType::Specular
                        };

                        let bounce = Bounce {
                            ty: bounce_type,
                            dispersed: dispersed || normal_dispersed,
                            color: component.bsdf.color,
                            incident: ray.direction,
                            position,
                            normal,
                            texture: surface_data.texture,
                            probability: probability * component_probability,
                            direct_light,
                        };

                        ray = Ray3::new(position, out_direction);
                        path.push(bounce);
                    }
                    Scattering::Emitted => {
                        if sample_light {
                            path.push(Bounce {
                                ty: BounceType::Emission,
                                dispersed: normal_dispersed,
                                color: component.bsdf.color,
                                incident: ray.direction,
                                position,
                                normal,
                                texture: surface_data.texture,
                                probability: component_probability,
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
                    dispersed: false,
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
    wavelength: f32,
    ray_in: Vector3<f32>,
    position: Point3<f32>,
    normal: Vector3<f32>,
    world: &'w World,
    brdf: Brdf,
    exe: &mut ExecutionContext<'w>,
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
                        let (color, material_probability, dispersed, target_normal) = match surface
                        {
                            lamp::Surface::Physical {
                                normal: target_normal,
                                material,
                                texture,
                            } => {
                                let component = material.choose_emissive(rng);
                                let input = ProbabilityInput {
                                    wavelength,
                                    wavelength_used: Cell::new(false),
                                    normal: target_normal,
                                    incident: ray_out.direction,
                                    texture_coordinate: texture,
                                };

                                let probability = component.get_probability(exe, &input);

                                (
                                    component.bsdf.color,
                                    probability,
                                    input.wavelength_used.get(),
                                    target_normal,
                                )
                            }
                            lamp::Surface::Color(color) => {
                                let target_normal = -ray_out.direction;
                                (color, 1.0, false, target_normal)
                            }
                        };
                        let scale = weight * probability * brdf(ray_in, normal, ray_out.direction);

                        return Some(DirectLight {
                            dispersed,
                            color,
                            incident: ray_out.direction,
                            normal: target_normal,
                            probability: scale * material_probability,
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
