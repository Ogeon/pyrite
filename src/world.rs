use std::sync::Arc;
use std::io::BufReader;
use std::fs::File;
use std::path::Path;
use std::collections::HashMap;

use rand::Rng;

use obj;
use genmesh;

use cgmath::{EuclideanVector, Vector3, Point, Point3, Ray3};

use config::Prelude;
use config::entry::Entry;

use bkdtree;

use tracer::{self, Material, ParametricValue, Color};
use lamp::Lamp;
use shapes::{self, Shape};
use materials;

pub enum Sky {
    Color(Box<Color>)
}

impl Sky {
    pub fn color<'a>(&'a self, _direction: &Vector3<f64>) -> &'a Color {
        match *self {
            Sky::Color(ref c) => & **c,
        }
    }
}

pub struct World {
    pub sky: Sky,
    pub lights: Vec<Lamp>,
    pub objects: Box<ObjectContainer + 'static + Send + Sync>
}

impl World {
    pub fn intersect(&self, ray: &Ray3<f64>) -> Option<(Ray3<f64>, &Material)> {
        self.objects.intersect(ray)
    }

    pub fn pick_lamp<R: Rng>(&self, rng: &mut R) -> Option<(&Lamp, f64)> {
        self.lights.get(rng.gen_range(0, self.lights.len())).map(|l| (l, 1.0 / self.lights.len() as f64))
    }
}

pub enum Object {
    Lamp(Lamp),
    Shape { shape: Shape, emissive: bool },
    Mesh { file: String, materials: HashMap<String, (materials::MaterialBox, bool)> },
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

pub fn register_types(context: &mut Prelude) {
    let mut group = context.object("Sky".into());
    let mut object = group.object("Color".into());
    object.add_decoder(decode_sky_color);
    object.arguments(vec!["color".into()]);
}

fn decode_sky_color(entry: Entry) -> Result<Sky, String> {
    let fields = try!(entry.as_object().ok_or("not an object".into()));

    let color = match fields.get("color") {
        Some(v) => try!(tracer::decode_parametric_number(v), "color"),
        None => return Err("missing field 'color'".into())
    };

    Ok(Sky::Color(color))
}

pub fn decode_world<F: Fn(String) -> P, P: AsRef<Path>>(entry: Entry, make_path: F) -> Result<World, String> {
    let fields = try!(entry.as_object().ok_or("not an object".into()));

    let sky = match fields.get("sky") {
        Some(v) => try!(v.dynamic_decode(), "sky"),
        None => return Err("missing field 'sky'".into())
    };

    let object_protos = match fields.get("objects") {
        Some(v) => try!(v.as_list().ok_or(String::from("expected a list")), "objects"),
        None => return Err("missing field 'objects'".into())
    };

    let mut objects: Vec<Arc<shapes::Shape>> = Vec::new();
    let mut lights = Vec::new();

    for (i, object) in object_protos.into_iter().enumerate() {
        let shape = try!(object.dynamic_decode(), format!("objects: [{}]", i));
        match shape {
            Object::Shape { shape, emissive } => {
                let shape = Arc::new(shape);
                if emissive {
                    lights.push(Lamp::Shape(shape.clone()));
                }
                objects.push(shape);
            },
            Object::Mesh { file, mut materials } => {
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
                        Some(m) => {
                            let (material, emissive): (Box<Material + 'static + Send + Sync>, bool) = m;
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
                                        lights.push(Lamp::Shape(triangle.clone()));
                                    }

                                    objects.push(triangle);
                                },
                                _ => {}
                            }
                        }
                    }
                }
            },
            Object::Lamp(light) => lights.push(light)
        }
    }

    println!("the scene contains {} objects", objects.len());
    println!("building BKD-Tree... ");
    let tree = bkdtree::BkdTree::new(objects, 3, 10); //TODO: make arrity configurable
    println!("done building BKD-Tree");
    Ok(World {
        sky: sky,
        lights: lights,
        objects: Box::new(tree) as Box<ObjectContainer + 'static + Send + Sync>
    })
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
