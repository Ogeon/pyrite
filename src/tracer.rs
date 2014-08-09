use std::rand::Rng;
use std::iter::Iterator;

use cgmath::vector::Vector3;
use cgmath::ray::Ray3;

pub trait World {
    fn intersect(&self, ray: &Ray3<f64>) -> Option<(Ray3<f64>, f64, &Material)>;
    fn sky_color(&self, direction: &Vector3<f64>) -> &ParametricValue<f64, f64>;
}

pub trait Material {
    fn reflect(&self, ray_in: &Ray3<f64>, normal: &Ray3<f64>, rng: &mut FloatRng) -> Reflection;
}

impl Material for Box<Material + Send + Sync> {
    fn reflect(&self, ray_in: &Ray3<f64>, normal: &Ray3<f64>, rng: &mut FloatRng) -> Reflection {
        self.reflect(ray_in, normal, rng)
    }
}

pub trait ParametricValue<From, To> {
    fn get(&self, i: From) -> To; 
}

impl ParametricValue<f64, f64> for f64 {
    fn get(&self, _: f64) -> f64 {
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
    Emit(&'a ParametricValue<f64, f64>),
    Reflect(Ray3<f64>, &'a ParametricValue<f64, f64>)
}


pub fn trace<R: Rng + FloatRng, W: World>(rng: &mut R, ray: Ray3<f64>, frequency: f64, world: &W, bounces: uint) -> f64 {
    let mut path = Vec::new();

    let mut ray = ray;

    for i in range(0, bounces) {
        match world.intersect(&ray) {
            Some((normal, _distance, material)) => match material.reflect(&ray, &normal, &mut *rng as &mut FloatRng) {
                Reflect(out_ray, brightness) => {
                    path.push(brightness);
                    ray = out_ray;
                },
                Emit(brightness) => {
                    return evaluate_contribution(frequency, brightness, path)
                }
            },
            None => break
        };
    }

    evaluate_contribution(frequency, world.sky_color(&ray.direction), path)
}

pub fn evaluate_contribution(frequency: f64, sky_color: &ParametricValue<f64, f64>, path: Vec<&ParametricValue<f64, f64>>) -> f64 {
    path.iter().rev().fold(sky_color.get(frequency), |prod, v| prod * v.get(frequency))
}