use std;
use std::sync::Arc;
use std::collections::HashMap;
use std::fs::File;
use std::io::BufReader;
use std::path::Path;

use rand::Rng;

use cgmath::{Vector, EuclideanVector, Vector3};
use cgmath::{Ray, Ray3};
use cgmath::{Point, Point3};

use obj;
use genmesh;

use config;
use shapes;
use bkdtree;

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

pub trait ObjectContainer {
    fn intersect(&self, ray: &Ray3<f64>) -> Option<(Ray3<f64>, &Material)>;
}

impl<'a> ObjectContainer for bkdtree::BkdTree<BkdRay<'a>, Arc<shapes::Shape>> {
    fn intersect(&self, ray: &Ray3<f64>) -> Option<(Ray3<f64>, &Material)> {
        let ray = BkdRay(ray);
        self.find(&ray).map(|(normal, object)| (normal, object.get_material()))
    }
}

pub struct BkdRay<'a>(pub &'a Ray3<f64>);

impl<'a> bkdtree::Ray for BkdRay<'a> {
    fn plane_intersections(&self, min: f64, max: f64, axis: usize) -> Option<(f64, f64)> {
        let &BkdRay(ray) = self;

        let (origin, direction) = match axis {
            0 => (ray.origin.x, ray.direction.x),
            1 => (ray.origin.y, ray.direction.y),
            _ => (ray.origin.z, ray.direction.z)
        };

        let min = (min - origin) / direction;
        let max = (max - origin) / direction;
        let far = min.max(max);

        if far > 0.0 {
            let near = min.min(max);
            Some((near, far))
        } else {
            None
        }
    }

    #[inline]
    fn plane_distance(&self, min: f64, max: f64, axis: usize) -> (f64, f64) {
        let &BkdRay(ray) = self;

        let (origin, direction) = match axis {
            0 => (ray.origin.x, ray.direction.x),
            1 => (ray.origin.y, ray.direction.y),
            _ => (ray.origin.z, ray.direction.z)
        };
        let min = (min - origin) / direction;
        let max = (max - origin) / direction;
        
        if min < max {
            (min, max)
        } else {
            (max, min)
        }
    }
}

pub enum Sky {
    Color(Box<ParametricValue<RenderContext, f64>>)
}

impl Sky {
    pub fn color(&self, _direction: &Vector3<f64>) -> &ParametricValue<RenderContext, f64> {
        match *self {
            Sky::Color(ref c) => & **c,
        }
    }
}

pub struct World {
    pub sky: Sky,
    pub lights: Vec<Arc<shapes::Shape>>,
    pub objects: Box<ObjectContainer + 'static + Send + Sync>
}

impl World {
    fn intersect(&self, ray: &Ray3<f64>) -> Option<(Ray3<f64>, &Material)> {
        self.objects.intersect(ray)
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

                    brdf.map(|brdf| {
                        let direct_light = trace_direct(rng, light_samples, &wavelengths, &ray.direction, &normal, world, brdf);

                        for (sample, light_sum) in traced.iter_mut().zip(direct_light.into_iter()) {
                            if light_sum > 0.0 {
                                sample.brightness += sample.reflectance * light_sum;
                                sample.sample_light = false;
                            } else {
                                sample.sample_light = true;
                            }
                        }
                    });


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
                                    
                                    brdf.map(|brdf| {
                                        let direct_light = trace_direct(rng, light_samples, &[sample.wavelength], &ray.direction, &normal, world, brdf);
                                        let light_sum = direct_light[0];

                                        if light_sum > 0.0 {
                                            sample.brightness += sample.reflectance * light_sum;
                                            sample.sample_light = false;
                                        } else {
                                            sample.sample_light = true;
                                        }
                                    });

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
                let sky = world.sky.color(&ray.direction);
                for mut sample in traced.into_iter() {
                    let context = RenderContext {
                        wavelength: sample.wavelength,
                        normal: Vector3::new(0.0, 0.0, 0.0),
                        incident: ray.direction
                    };

                    sample.brightness += sample.reflectance * sky.get(&context);
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

                            brdf.map(|brdf| {
                                let direct_light = trace_direct(rng, light_samples, &wl, &ray.direction, &normal, world, brdf);
                                let light_sum = direct_light[0];
                                
                                if light_sum > 0.0 {
                                    sample.brightness += sample.reflectance * light_sum;
                                    sample.sample_light = false;
                                } else {
                                    sample.sample_light = true;
                                }
                            });

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
                let sky = world.sky.color(&ray.direction);
                
                let context = RenderContext {
                    wavelength: sample.wavelength,
                    normal: Vector3::new(0.0, 0.0, 0.0),
                    incident: ray.direction
                };

                sample.brightness += sample.reflectance * sky.get(&context);
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
        let target_normal = match light.sample_point(rng) {
            Some(normal) => normal,
            None => return sum
        };

        let ray_out = target_normal.origin.sub_p(&normal.origin);

        let distance = ray_out.length2();
        let ray_out = Ray::new(normal.origin, ray_out.normalize());

        let cos_out = normal.direction.dot(&ray_out.direction).max(0.0);
        let cos_in = target_normal.direction.dot(& -ray_out.direction).abs();

        if cos_out > 0.0 {
            let color = light.get_material().get_emission(wavelengths, &ray_out.direction, &target_normal, &mut *rng as &mut FloatRng);
            let scale = weight * cos_in * brdf(ray_in, &normal.direction, &ray_out.direction) / distance;

            color.map(|color| match world.intersect(&ray_out) {
                None => for (&wavelength, sum) in wavelengths.iter().zip(sum.iter_mut()) {
                    let context = RenderContext {
                        wavelength: wavelength,
                        normal: target_normal.direction,
                        incident: ray_out.direction
                    };

                    *sum += color.get(&context) * scale;
                },
                Some((hit_normal, _)) if hit_normal.origin.sub_p(&normal.origin).length2() >= distance - 0.0000001
                  => for (&wavelength, sum) in wavelengths.iter().zip(sum.iter_mut()) {
                    let context = RenderContext {
                        wavelength: wavelength,
                        normal: target_normal.direction,
                        incident: ray_out.direction
                    };

                    *sum += color.get(&context) * scale;
                },
                _ => {}
            });
        }
        
        sum
    })
}



pub fn register_types(context: &mut config::ConfigContext) {
    context.insert_grouped_type("Sky", "Color", decode_sky_color);
}

fn decode_sky_color(context: &config::ConfigContext, fields: HashMap<String, config::ConfigItem>) -> Result<Sky, String> {
    let mut fields = fields;

    let color = match fields.remove("color") {
        Some(v) => try!(decode_parametric_number(context, v), "color"),
        None => return Err("missing field 'color'".into())
    };

    Ok(Sky::Color(color))
}

pub fn decode_world<F: Fn(String) -> P, P: AsRef<Path>>(context: &config::ConfigContext, item: config::ConfigItem, make_path: F) -> Result<World, String> {
    match item {
        config::Structure(_, mut fields) => {
            let sky = match fields.remove("sky") {
                Some(v) => try!(context.decode_structure_from_group("Sky", v), "sky"),
                None => return Err("missing field 'sky'".into())
            };

            let object_protos = match fields.remove("objects") {
                Some(v) => try!(v.into_list(), "objects"),
                None => return Err("missing field 'objects'".into())
            };

            let mut objects: Vec<Arc<shapes::Shape>> = Vec::new();
            let mut lights: Vec<Arc<shapes::Shape>> = Vec::new();

            for (i, object) in object_protos.into_iter().enumerate() {
                let shape: shapes::ProxyShape = try!(context.decode_structure_from_group("Shape", object), format!("objects: [{}]", i));
                match shape {
                    shapes::DecodedShape { shape, emissive } => {
                        let shape = Arc::new(shape);
                        if emissive {
                            lights.push(shape.clone());
                        }
                        objects.push(shape);
                    },
                    shapes::Mesh { file, mut materials } => {
                        let path = make_path(file);
                        let file = match File::open(&path) {
                            Ok(f) => f,
                            Err(e) => return Err(format!("failed to open {}: {}", path.as_ref().display(), e))
                        };
                        let mut file = BufReader::new(file);
                        let obj = obj::Obj::load(&mut file);
                        for object in obj.object_iter() {
                            println!("adding object '{}'", object.name);
                            
                            let (object_material, emissive) = match materials.remove(&object.name) {
                                Some(v) => {
                                    let (material, emissive): (Box<Material + 'static + Send + Sync>, bool) =
                                        try!(context.decode_structure_from_group("Material", v));

                                    (Arc::new(material), emissive)
                                },
                                None => return Err(format!("objects: [{}]: missing field '{}'", i, object.name))
                            };

                            for group in object.group_iter() {
                                for shape in group.indices().iter() {
                                    match *shape {
                                        genmesh::Polygon::PolyTri(genmesh::Triangle{x, y, z}) => {
                                            let triangle = Arc::new(make_triangle(&obj, x, y, z, object_material.clone()));

                                            if emissive {
                                                lights.push(triangle.clone());
                                            }

                                            objects.push(triangle);
                                        },
                                        _ => {}
                                    }
                                }
                            }
                        }
                    }
                }
            }

            println!("the scene contains {} objects", objects.len());
            println!("building BKD-Tree... ");
            let tree = bkdtree::BkdTree::new(objects, 3);
            println!("done building BKD-Tree");
            Ok(World {
                sky: sky,
                lights: lights,
                objects: Box::new(tree) as Box<ObjectContainer + 'static + Send + Sync>
            })
        },
        config::Primitive(v) => Err(format!("unexpected {:?}", v)),
        config::List(_) => Err(format!("unexpected list"))
    }
}

fn vertex_to_point(v: &[f32; 3]) -> Point3<f64> {
    Point3::new(v[0] as f64, v[1] as f64, v[2] as f64)
}

fn vertex_to_vector(v: &[f32; 3]) -> Vector3<f64> {
    Vector3::new(v[0] as f64, v[1] as f64, v[2] as f64)
}

fn make_triangle<M>(
    obj: &obj::Obj<M>,
    (v1, _t1, n1): (usize, Option<usize>, Option<usize>),
    (v2, _t2, n2): (usize, Option<usize>, Option<usize>),
    (v3, _t3, n3): (usize, Option<usize>, Option<usize>),
    material: Arc<Box<Material + 'static + Send + Sync>>
) -> shapes::Shape {
    let v1 = vertex_to_point(&obj.position()[v1]);
    let v2 = vertex_to_point(&obj.position()[v2]);
    let v3 = vertex_to_point(&obj.position()[v3]);

    let (n1, n2, n3) = match (n1, n2, n3) {
        (Some(n1), Some(n2), Some(n3)) => {
            let n1 = vertex_to_vector(&obj.normal()[n1]);
            let n2 = vertex_to_vector(&obj.normal()[n2]);
            let n3 = vertex_to_vector(&obj.normal()[n3]);
            (n1, n2, n3)
        },
        _ => {
            let a = v2.sub_p(&v1);
            let b = v3.sub_p(&v1);
            let normal = a.cross(&b).normalize();
            (normal, normal, normal)
        }
    };

    shapes::Triangle {
        v1: shapes::Vertex { position: v1, normal: n1 },
        v2: shapes::Vertex { position: v2, normal: n2 },
        v3: shapes::Vertex { position: v3, normal: n3 },
        material: material
    }
}

pub fn decode_parametric_number<From: 'static>(context: &config::ConfigContext, item: config::ConfigItem) -> Result<Box<ParametricValue<From, f64>>, String> {
    let group_names = vec!["Math", "Value"];

    let name_collection = if group_names.len() == 1 {
        format!("'{}'", group_names.first().unwrap())
    } else if group_names.len() > 1 {
        let names = &group_names[..group_names.len() - 1];
        format!("'{}' or '{}'", names.connect("', '"), group_names.last().unwrap())
    } else {
        return Err("internal error: trying to decode structure from one of 0 groups".into())
    };

    match item {
        config::Structure(..) => context.decode_structure_from_groups(group_names, item),
        config::Primitive(config::parser::Value::Number(n)) => Ok(Box::new(n) as Box<ParametricValue<From, f64>>),
        v => return Err(format!("expected a number or a structure from group {}, but found {}", name_collection, v))
    }
}