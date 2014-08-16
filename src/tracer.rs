use std::rand::Rng;
use std::sync::Arc;
use std::iter::Iterator;
use std::collections::HashMap;
use std::io::File;
use std::simd;

use cgmath::vector::{EuclideanVector, Vector3};
use cgmath::ray::Ray3;
use cgmath::point::{Point, Point3};

use obj::obj;
use config;
use shapes;
use bkdtree;

pub trait Material {
    fn reflect(&self, ray_in: &Ray3<f64>, normal: &Ray3<f64>, rng: &mut FloatRng) -> Reflection;
}

impl Material for Box<Material + 'static + Send + Sync> {
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
    fn intersect(&self, ray: &Ray3<f64>) -> Option<(Ray3<f64>, &Material)>;
}

impl ObjectContainer for Vec<shapes::Shape> {
    fn intersect(&self, ray: &Ray3<f64>) -> Option<(Ray3<f64>, &Material)> {
        let mut closest: Option<(Ray3<f64>, f64, &Material)> = None;

        for object in self.iter() {
            closest = object.intersect(ray).map(|(new_dist, normal)| {
                match closest {
                    Some((closest_normal, closest_dist, closest_material)) => {
                        if new_dist > 0.000001 && new_dist < closest_dist {
                            (normal, new_dist, object.get_material())
                        } else {
                            (closest_normal, closest_dist, closest_material)
                        }
                    },
                    None => (normal, new_dist, object.get_material())
                }

            }).or(closest);
        }

        closest.map(|(normal, _, material)| (normal, material))
    }
}

impl ObjectContainer for bkdtree::BkdTree<shapes::Shape> {
    fn intersect(&self, ray: &Ray3<f64>) -> Option<(Ray3<f64>, &Material)> {
        let ray = BkdRay(ray);
        self.find(&ray).map(|(normal, object)| (normal, object.get_material()))
    }
}

pub struct BkdRay<'a>(pub &'a Ray3<f64>);

impl<'a> bkdtree::Ray for BkdRay<'a> {
    fn plane_intersections(&self, min: f64, max: f64, axis: uint) -> Option<(f64, f64)> {
        let &BkdRay(ray) = self;

        let (origin, direction) = match axis {
            0 => (simd::f64x2(ray.origin.x, ray.origin.x), simd::f64x2(ray.direction.x, ray.direction.x)),
            1 => (simd::f64x2(ray.origin.y, ray.origin.y), simd::f64x2(ray.direction.y, ray.direction.y)),
            _ => (simd::f64x2(ray.origin.z, ray.origin.z), simd::f64x2(ray.direction.z, ray.direction.z))
        };

        let plane = simd::f64x2(min, max);
        let simd::f64x2(min, max) = (plane - origin) / direction;
        let far = min.max(max);

        if far > 0.0 {
            let near = min.min(max);
            Some((near, far))
        } else {
            None
        }
    }

    #[inline]
    fn plane_distance(&self, min: f64, max: f64, axis: uint) -> (f64, f64) {
        let &BkdRay(ray) = self;

        let (origin, direction) = match axis {
            0 => (simd::f64x2(ray.origin.x, ray.origin.x), simd::f64x2(ray.direction.x, ray.direction.x)),
            1 => (simd::f64x2(ray.origin.y, ray.origin.y), simd::f64x2(ray.direction.y, ray.direction.y)),
            _ => (simd::f64x2(ray.origin.z, ray.origin.z), simd::f64x2(ray.direction.z, ray.direction.z))
        };

        let plane = simd::f64x2(min, max);
        let simd::f64x2(min, max) = (plane - origin) / direction;
        
        if min < max {
            (min, max)
        } else {
            (max, min)
        }
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
    fn intersect(&self, ray: &Ray3<f64>) -> Option<(Ray3<f64>, &Material)> {
        self.objects.intersect(ray)
    }
}

pub enum Reflection<'a> {
    Emit(&'a ParametricValue<RenderContext, f64>),
    Reflect(Ray3<f64>, &'a ParametricValue<RenderContext, f64>)
}

pub struct RenderContext {
    pub wavelength: f64,
    pub normal: Vector3<f64>,
    pub incident: Vector3<f64>
}

pub struct WavelengthSample {
    pub wavelength: f64,
    pub brightness: f64
}

pub fn trace<R: Rng + FloatRng>(rng: &mut R, ray: Ray3<f64>, wavelengths: Vec<f64>, world: &World, bounces: uint) -> Vec<WavelengthSample> {
    let mut ray = ray;

    let mut traced: Vec<(f64, f64)> = wavelengths.move_iter().map(|wl| (wl, 1.0)).collect();
    let mut completed = Vec::new();

    for i in range(0, bounces) {
        match world.intersect(&ray) {
            Some((normal, material)) => match material.reflect(&ray, &normal, &mut *rng as &mut FloatRng) {
                Reflect(out_ray, color) => {
                    let mut i = 0;
                    while i < traced.len() {
                        let (wl, brightness) = traced[i];
                        let context = RenderContext {
                            wavelength: wl,
                            normal: normal.direction,
                            incident: ray.direction
                        };

                        let brightness = brightness * color.get(&context);

                        if brightness == 0.0 {
                            traced.swap_remove(i);
                            completed.push(WavelengthSample {
                                wavelength: wl,
                                brightness: 0.0
                            });
                        } else {
                            let &(_, ref mut b) = traced.get_mut(i);
                            *b = brightness;
                            i += 1;
                        }
                    }

                    ray = out_ray;
                },
                Emit(color) => {
                    for (wl, brightness) in traced.move_iter() {
                        let context = RenderContext {
                            wavelength: wl,
                            normal: normal.direction,
                            incident: ray.direction
                        };

                        completed.push(WavelengthSample {
                            wavelength: wl,
                            brightness: brightness * color.get(&context)
                        });
                    }

                    return completed
                }
            },
            None => {
                let sky = world.sky.color(&ray.direction);
                for (wl, brightness) in traced.move_iter() {
                    let context = RenderContext {
                        wavelength: wl,
                        normal: Vector3::new(0.0, 0.0, 0.0),
                        incident: ray.direction
                    };

                    completed.push(WavelengthSample {
                        wavelength: wl,
                        brightness: brightness * sky.get(&context)
                    });
                }

                return completed
            }
        };
    }

    for (wl, brightness) in traced.move_iter() {
        completed.push(WavelengthSample {
            wavelength: wl,
            brightness: 0.0
        });
    }

    completed
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

pub fn decode_world(context: &config::ConfigContext, item: config::ConfigItem, make_path: |String| -> Path) -> Result<World, String> {
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
                let shape: shapes::ProxyShape = try!(context.decode_structure_from_group("Shape", object), format!("objects: [{}]", i));
                match shape {
                    shapes::DecodedShape { shape } => objects.push(shape),
                    shapes::Mesh { file, mut materials } => {
                        let path = make_path(file);
                        let file = match File::open(&path).read_to_string() {
                            Ok(obj) => obj,
                            Err(e) => return Err(format!("objects: [{}]: unable to open file '{}': {}", i, path.display(), e))
                        };

                        let obj = match obj::parse(file) {
                            Ok(obj) => obj,
                            Err(e) => return Err(format!("objects: [{}]: parse file '{}': line {}: {}", i, path.display(), e.line_number, e.message))
                        };

                        for object in obj.objects.iter() {
                            let object_material: Arc<Box<Material + 'static + Send + Sync>> =
                                match materials.pop_equiv(&object.name) {
                                    Some(v) => Arc::new(try!(context.decode_structure_from_group("Material", v))),
                                    None => return Err(format!("objects: [{}]: missing field '{}'", i, object.name))
                                };

                            for group in object.geometry.iter() {
                                for shape in group.shapes.iter() {
                                    match *shape {
                                        obj::Triangle((v1, _t1), (v2, _t2), (v3, _t3)) => {
                                            let triangle = shapes::Triangle {
                                                v1: convert_vertex(object.verticies[v1]),
                                                v2: convert_vertex(object.verticies[v2]),
                                                v3: convert_vertex(object.verticies[v3]),
                                                material: object_material.clone()
                                            };

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

            println!("the scene contains {} objects", objects.len())

            Ok(World {
                sky: sky,
                objects: box bkdtree::BkdTree::new(objects, 3) as Box<ObjectContainer + 'static + Send + Sync>
            })
        },
        config::Primitive(v) => Err(format!("unexpected {}", v)),
        config::List(_) => Err(format!("unexpected list"))
    }
}

fn convert_vertex(vec: obj::Vertex) -> Point3<f64> {
    Point3::new(vec.x, vec.y, vec.z)
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