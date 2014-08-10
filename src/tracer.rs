use std::rand::Rng;
use std::iter::Iterator;
use std::collections::HashMap;

use cgmath::vector::{EuclideanVector, Vector3};
use cgmath::ray::Ray3;
use cgmath::point::Point;

use config;

use shapes;

pub trait Material {
    fn reflect(&self, ray_in: &Ray3<f64>, normal: &Ray3<f64>, rng: &mut FloatRng) -> Reflection;
}

impl Material for Box<Material + Send + Sync> {
    fn reflect(&self, ray_in: &Ray3<f64>, normal: &Ray3<f64>, rng: &mut FloatRng) -> Reflection {
        self.reflect(ray_in, normal, rng)
    }
}

pub trait ParametricValue<From, To> {
    fn get(&self, i: &From) -> To; 
}

impl<From, To> ParametricValue<From, To> for Box<ParametricValue<From, To> + 'static + Send + Sync> {
    fn get(&self, i: &From) -> To {
        self.get(i)
    }
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

pub trait ObjectContainer {
    fn intersect(&self, ray: &Ray3<f64>) -> Option<(Ray3<f64>, f64, &Material)>;
}

impl ObjectContainer for Vec<shapes::Shape> {
    fn intersect(&self, ray: &Ray3<f64>) -> Option<(Ray3<f64>, f64, &Material)> {
        let mut closest: Option<(Ray3<f64>, f64, &Material)> = None;

        for object in self.iter() {
            closest = object.intersect(ray).map(|(normal, material)| {

                let new_dist = ray.origin.sub_p(&normal.origin).length2();

                match closest {
                    Some((closest_normal, closest_dist, closest_material)) => {
                        if new_dist > 0.000001 && new_dist < closest_dist {
                            (normal, new_dist, material as &Material)
                        } else {
                            (closest_normal, closest_dist, closest_material)
                        }
                    },
                    None => (normal, new_dist, material as &Material)
                }

            }).or(closest);
        }

        closest
    }
}

pub enum Sky {
    Color(Box<ParametricValue<RenderContext, f64> + 'static + Send + Sync>)
}

impl Sky {
    pub fn color(&self, _direction: &Vector3<f64>) -> &ParametricValue<RenderContext, f64> {
        match *self {
            Color(ref c) => c as &ParametricValue<RenderContext, f64>,
        }
    }
}

pub struct World {
    pub sky: Sky,
    pub objects: Box<ObjectContainer + 'static + Send + Sync>
}

impl World {
    fn intersect(&self, ray: &Ray3<f64>) -> Option<(Ray3<f64>, f64, &Material)> {
        self.objects.intersect(ray)
    }
}

pub enum Reflection<'a> {
    Emit(&'a ParametricValue<RenderContext, f64>),
    Reflect(Ray3<f64>, &'a ParametricValue<RenderContext, f64>)
}

pub struct RenderContext {
    pub frequency: f64
}


pub fn trace<R: Rng + FloatRng>(rng: &mut R, ray: Ray3<f64>, frequency: f64, world: &World, bounces: uint) -> f64 {
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
                    let context = RenderContext {
                        frequency: frequency
                    };

                    return evaluate_contribution(&context, brightness, path)
                }
            },
            None => break
        };
    }

    let context = RenderContext {
        frequency: frequency
    };
    evaluate_contribution(&context, world.sky.color(&ray.direction), path)
}

pub fn evaluate_contribution(context: &RenderContext, sky_color: &ParametricValue<RenderContext, f64>, path: Vec<&ParametricValue<RenderContext, f64>>) -> f64 {
    path.iter().rev().fold(sky_color.get(context), |prod, v| prod * v.get(context))
}




pub fn register_types(context: &mut config::ConfigContext) {
    context.insert_grouped_type("Sky", "Color", decode_sky_color);
}

fn decode_sky_color(context: &config::ConfigContext, fields: HashMap<String, config::ConfigItem>) -> Result<Sky, String> {
    let mut fields = fields;

    let color = match fields.pop_equiv(&"color") {
        Some(v) => try!(decode_parametric_number(context, v), "color"),
        None => return Err(String::from_str("missing field 'color'"))
    };

    Ok(Color(color))
}

pub fn decode_world(context: &config::ConfigContext, item: config::ConfigItem) -> Result<World, String> {
    match item {
        config::Structure(_, mut fields) => {
            let sky = match fields.pop_equiv(&"sky") {
                Some(v) => try!(context.decode_structure_from_group("Sky", v), "sky"),
                None => return Err(String::from_str("missing field 'sky'"))
            };

            let object_protos = match fields.pop_equiv(&"objects") {
                Some(v) => try!(v.into_list(), "objects"),
                None => return Err(String::from_str("missing field 'objects'"))
            };

            let mut objects: Vec<shapes::Shape> = Vec::new();

            for (i, object) in object_protos.move_iter().enumerate() {
                objects.push(try!(context.decode_structure_from_group("Shape", object), format!("[{}]", i)))
            }

            Ok(World {
                sky: sky,
                objects: box objects as Box<ObjectContainer + 'static + Send + Sync>
            })
        },
        config::Primitive(v) => Err(format!("unexpected {}", v)),
        config::List(_) => Err(format!("unexpected list"))
    }
}

pub fn decode_parametric_number<From>(context: &config::ConfigContext, item: config::ConfigItem) -> Result<Box<ParametricValue<From, f64> + 'static + Send + Sync>, String> {
    let group_names = vec!["Math", "Value"];

    let name_collection = match group_names.as_slice() {
        [name] => format!("'{}'", name),
        [..names, last] => format!("'{}' or '{}'", names.connect("', '"), last),
        [] => return Err(String::from_str("internal error: trying to decode structure from one of 0 groups"))
    };

    match item {
        config::Structure(..) => context.decode_structure_from_groups(group_names, item),
        config::Primitive(config::Number(n)) => Ok(box n as Box<ParametricValue<From, f64> + 'static + Send + Sync>),
        v => return Err(format!("expected a number or a structure from group {}, but found {}", name_collection, v))
    }
}