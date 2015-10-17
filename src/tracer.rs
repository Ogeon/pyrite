use std;

use rand::Rng;

use cgmath::{Vector, EuclideanVector, Vector3};
use cgmath::{Ray, Ray3};
use cgmath::Point;

use config::{Value, Decode};
use config::entry::Entry;
use lamp::{self, Lamp};
use world::World;

pub use self::Reflection::{Reflect, Emit, Disperse};

pub type Brdf = fn(ray_in: &Vector3<f64>, ray_out: &Vector3<f64>, normal: &Vector3<f64>) -> f64;

pub trait Material {
    fn reflect(&self, wavelengths: &[f64], ray_in: &Ray3<f64>, normal: &Ray3<f64>, rng: &mut FloatRng) -> Reflection;
    fn get_emission(&self, wavelengths: &[f64], ray_in: &Vector3<f64>, normal: &Ray3<f64>, rng: &mut FloatRng) -> Option<&ParametricValue<RenderContext, f64>>;
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
    Emit(&'a ParametricValue<RenderContext, f64>),
    Reflect(Ray3<f64>, &'a ParametricValue<RenderContext, f64>, f64, Option<Brdf>),
    Disperse(Vec<Reflection<'a>>)
}

pub struct RenderContext {
    pub wavelength: f64,
    pub normal: Vector3<f64>,
    pub incident: Vector3<f64>
}

pub struct WavelengthSample {
    pub wavelength: f64,
    reflectance: f64,
    pub brightness: f64,
    pub weight: f64,
    sample_light: bool
}

pub fn trace<R: Rng + FloatRng>(rng: &mut R, ray: Ray3<f64>, wavelengths: Vec<f64>, world: &World, bounces: u32, light_samples: usize) -> Vec<WavelengthSample> {
    let mut ray = ray;

    let mut wavelengths = wavelengths;
    let mut traced: Vec<WavelengthSample> = wavelengths.iter().map(|&wl| WavelengthSample {
        wavelength: wl,
        reflectance: 1.0,
        brightness: 0.0,
        weight: 1.0,
        sample_light: true
    }).collect();
    let mut completed = Vec::new();

    for bounce in 0..bounces {
        match world.intersect(&ray) {
            Some((normal, material)) => match material.reflect(&wavelengths, &ray, &normal, &mut *rng as &mut FloatRng) {
                Reflect(out_ray, color, scale, brdf) => {
                    for sample in traced.iter_mut() {
                        let context = RenderContext {
                            wavelength: sample.wavelength,
                            normal: normal.direction,
                            incident: ray.direction
                        };

                        sample.reflectance *= color.get(&context) * scale;
                    }

                    if let Some(brdf) = brdf {
                        let direct_light = trace_direct(rng, light_samples, &wavelengths, &ray.direction, &normal, world, brdf);
                        
                        for (sample, light_sum) in traced.iter_mut().zip(direct_light.into_iter()) {
                            if light_sum > 0.0 {
                                sample.brightness += sample.reflectance * light_sum;
                                sample.sample_light = false;
                            } else {
                                sample.sample_light = true;
                            }
                        }
                    }

                    let mut i = 0;
                    while i < traced.len() {
                        let WavelengthSample {reflectance, ..} = traced[i];

                        let brdf_scale = brdf.map(|brdf| brdf(&ray.direction, &normal.direction, &out_ray.direction)).unwrap_or(1.0);
                        let new_reflectance = reflectance * brdf_scale;

                        if new_reflectance == 0.0 {
                            let sample = traced.swap_remove(i);
                            wavelengths.swap_remove(i);
                            completed.push(sample);
                        } else {
                            let &mut WavelengthSample {ref mut reflectance, ref mut sample_light, ..} = &mut traced[i];
                            *reflectance = new_reflectance;
                            *sample_light = brdf.is_none() || *sample_light;
                            i += 1;
                        }
                    }

                    ray = out_ray;
                },
                Emit(color) => {
                    for mut sample in traced.into_iter() {
                        let context = RenderContext {
                            wavelength: sample.wavelength,
                            normal: normal.direction,
                            incident: ray.direction
                        };

                        if sample.sample_light {
                            sample.brightness += sample.reflectance * color.get(&context);
                        }
                        completed.push(sample);
                    }

                    return completed
                },
                Disperse(reflections) => {
                    let bounces = bounces - (bounce + 1);
                    for (mut sample, mut reflection) in traced.into_iter().zip(reflections.into_iter()) {
                        let context = RenderContext {
                            wavelength: sample.wavelength,
                            normal: normal.direction,
                            incident: ray.direction
                        };

                        loop {
                            match reflection {
                                Disperse(mut reflections) => reflection = reflections.pop().expect("internal error: no reflections"),
                                Reflect(out_ray, color, scale, brdf) => {
                                    sample.reflectance *= color.get(&context) * scale;

                                    if let Some(brdf) = brdf {
                                        let direct_light = trace_direct(rng, light_samples, &[sample.wavelength], &ray.direction, &normal, world, brdf);
                                        let light_sum = direct_light[0];

                                        if light_sum > 0.0 {
                                            sample.brightness += sample.reflectance * light_sum;
                                            sample.sample_light = false;
                                        } else {
                                            sample.sample_light = true;
                                        }
                                    }

                                    sample.reflectance *= brdf.map(|brdf| brdf(&ray.direction, &normal.direction, &out_ray.direction)).unwrap_or(1.0);
                                    sample.sample_light = brdf.is_none() || sample.sample_light;
                                    completed.push(trace_branch(rng, out_ray, sample, world, bounces, light_samples));
                                    break;
                                },
                                Emit(color) => {
                                    if sample.sample_light {
                                        sample.brightness += sample.reflectance * color.get(&context);
                                    }
                                    completed.push(sample);
                                    break;
                                }
                            }
                        }
                    }

                    return completed;
                }
            },
            None => {
                let direct_light = trace_directional(rng, &wavelengths, &ray.direction, world);
                let sky = world.sky.color(&ray.direction);

                for (mut sample, light) in traced.into_iter().zip(direct_light.into_iter()) {
                    let context = RenderContext {
                        wavelength: sample.wavelength,
                        normal: Vector3::new(0.0, 0.0, 0.0),
                        incident: ray.direction
                    };

                    sample.brightness += sample.reflectance * (sky.get(&context) + light);
                    completed.push(sample);
                }

                return completed
            }
        };
    }

    for sample in traced.into_iter() {
        completed.push(sample);
    }

    completed
}

fn trace_branch<R: Rng + FloatRng>(rng: &mut R, ray: Ray3<f64>, sample: WavelengthSample, world: &World, bounces: u32, light_samples: usize) -> WavelengthSample {
    let mut ray = ray;
    let mut sample = sample;
    let wl = [sample.wavelength];

    for _ in 0..bounces {
        match world.intersect(&ray) {
            Some((normal, material)) => {
                let mut reflection = material.reflect(&wl, &ray, &normal, &mut *rng as &mut FloatRng);
                loop {
                    match reflection {
                        Disperse(mut reflections) => reflection = reflections.pop().expect("internal error: no reflections in branch"),
                        Reflect(out_ray, color, scale, brdf) => {
                            let context = RenderContext {
                                wavelength: sample.wavelength,
                                normal: normal.direction,
                                incident: ray.direction
                            };

                            sample.reflectance *= color.get(&context) * scale;

                            if let Some(brdf) = brdf {
                                let direct_light = trace_direct(rng, light_samples, &wl, &ray.direction, &normal, world, brdf);
                                let light_sum = direct_light[0];
                                
                                if light_sum > 0.0 {
                                    sample.brightness += sample.reflectance * light_sum;
                                    sample.sample_light = false;
                                } else {
                                    sample.sample_light = true;
                                }
                            }


                            sample.reflectance *= brdf.map(|brdf| brdf(&ray.direction, &normal.direction, &out_ray.direction)).unwrap_or(1.0);
                            sample.sample_light = brdf.is_none() || sample.sample_light;

                            if sample.reflectance == 0.0 {
                                return sample;
                            }

                            ray = out_ray;
                            break;
                        },
                        Emit(color) => {
                            let context = RenderContext {
                                wavelength: sample.wavelength,
                                normal: normal.direction,
                                incident: ray.direction
                            };
                            if sample.sample_light {
                                sample.brightness += sample.reflectance * color.get(&context);
                            }
                            return sample;
                        }
                    }
                }
            },
            None => {
                let direct_light = trace_directional(rng, &[sample.wavelength], &ray.direction, world)[0];
                let sky = world.sky.color(&ray.direction);
                
                let context = RenderContext {
                    wavelength: sample.wavelength,
                    normal: Vector3::new(0.0, 0.0, 0.0),
                    incident: ray.direction
                };

                sample.brightness += sample.reflectance * (sky.get(&context) + direct_light);
                return sample
            }
        };
    }

    sample
}

fn trace_direct<'a, R: Rng + FloatRng>(rng: &mut R, samples: usize, wavelengths: &[f64], ray_in: &Vector3<f64>, normal: &Ray3<f64>, world: &'a World, brdf: Brdf) -> Vec<f64> {
    if world.lights.len() == 0 {
        return vec![0.0f64; samples];
    }

    let n = if ray_in.dot(&normal.direction) < 0.0 {
        normal.direction
    } else {
        -normal.direction
    };

    let normal = Ray::new(normal.origin, n);

    let ref light = world.lights[rng.gen_range(0, world.lights.len())];
    let weight = light.surface_area() * world.lights.len() as f64 / (samples as f64 * 2.0 * std::f64::consts::PI);

    (0..samples).fold(vec![0.0f64; samples], |mut sum, _| {
        let lamp::Sample {
            direction,
            sq_distance,
            surface
        } = light.sample(rng, normal.origin);

        let ray_out = Ray::new(normal.origin, direction);

        let cos_out = normal.direction.dot(&ray_out.direction).max(0.0);

        if cos_out > 0.0 {
            let hit_dist = world.intersect(&ray_out).map(|(hit_normal, _)| hit_normal.origin.sub_p(&normal.origin).length2());

            let blocked = match (hit_dist, sq_distance) {
                (Some(hit), Some(light)) if hit >= light - 0.0000001 => false,
                (None, _) => false,
                _ => true,
            };

            if !blocked {
                let (color, cos_in, target_normal) = match surface {
                    lamp::Surface::Physical {
                        normal: target_normal,
                        material
                    } => {
                        let color = material.get_emission(wavelengths, &ray_out.direction, &target_normal, &mut *rng as &mut FloatRng);
                        let cos_in = target_normal.direction.dot(& -ray_out.direction).abs();
                        (color, cos_in, target_normal.direction)
                    },
                    lamp::Surface::Color(color) => {
                        let target_normal = -ray_out.direction;
                        (Some(color), 1.0, target_normal)
                    },
                };
                let scale = weight * cos_in * brdf(ray_in, &normal.direction, &ray_out.direction) / sq_distance.unwrap_or(1.0);
                
                if let Some(color) = color {
                    for (&wavelength, sum) in wavelengths.iter().zip(sum.iter_mut()) {
                        let context = RenderContext {
                            wavelength: wavelength,
                            normal: target_normal,
                            incident: ray_out.direction
                        };

                        *sum += color.get(&context) * scale;
                    }
                }
            }
        }
        
        sum
    })
}


fn trace_directional<R: Rng>(rng: &mut R, wavelengths: &[f64], ray_in: &Vector3<f64>, world: &World) -> Vec<f64> {
    if let Some(light) = world.lights.get(rng.gen_range(0, world.lights.len())) {
        let weight = world.lights.len() as f64;

        if let &Lamp::Directional { direction, width, ref color } = light {
            if direction.dot(ray_in) >= width {
                return wavelengths.iter().map(|&wl| {
                    let context = RenderContext {
                        wavelength: wl,
                        normal: -direction,
                        incident: direction
                    };

                    color.get(&context) * weight
                }).collect();
            }
        }
    }
    
    vec![0.0f64; wavelengths.len()]
}

pub fn decode_parametric_number<From: Decode + 'static>(item: Entry) -> Result<Box<ParametricValue<From, f64>>, String> {
    if let Some(&Value::Number(num)) = item.as_value() {
        Ok(Box::new(num.as_float()) as Box<ParametricValue<From, f64>>)
    } else {
        item.dynamic_decode()
    }
}