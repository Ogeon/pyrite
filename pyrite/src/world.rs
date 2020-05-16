use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use rand::Rng;

use genmesh;
use obj;

use cgmath::{InnerSpace, Matrix4, Point3, Vector3};
use collision::Ray3;

use crate::config::entry::Entry;
use crate::config::Prelude;

use crate::spatial::{bkd_tree, Dim3};

use crate::lamp::Lamp;
use crate::materials;
use crate::shapes::{self, Shape};
use crate::tracer::{self, Color, Material};

pub enum Sky {
    Color(Box<Color>),
}

impl Sky {
    pub fn color<'a>(&'a self, _direction: &Vector3<f32>) -> &'a Color {
        match *self {
            Sky::Color(ref c) => &**c,
        }
    }
}

pub struct World<R: Rng> {
    pub sky: Sky,
    pub lights: Vec<Lamp<R>>,
    pub objects: Box<dyn ObjectContainer<R> + 'static + Send + Sync>,
}

impl<R: Rng> World<R> {
    pub fn intersect(&self, ray: &Ray3<f32>) -> Option<(Ray3<f32>, &dyn Material<R>)> {
        self.objects.intersect(ray)
    }

    pub fn pick_lamp(&self, rng: &mut R) -> Option<(&Lamp<R>, f32)> {
        self.lights
            .get(rng.gen_range(0, self.lights.len()))
            .map(|l| (l, 1.0 / self.lights.len() as f32))
    }
}

pub enum Object<R: Rng> {
    Lamp(Lamp<R>),
    Shape {
        shape: Shape<R>,
        emissive: bool,
    },
    Mesh {
        file: String,
        materials: HashMap<String, (materials::MaterialBox<R>, bool)>,
        scale: f32,
        transform: Matrix4<f32>,
    },
}

pub trait ObjectContainer<R: Rng> {
    fn intersect(&self, ray: &Ray3<f32>) -> Option<(Ray3<f32>, &dyn Material<R>)>;
}

impl<R: Rng> ObjectContainer<R> for bkd_tree::BkdTree<Arc<shapes::Shape<R>>> {
    fn intersect(&self, ray: &Ray3<f32>) -> Option<(Ray3<f32>, &dyn Material<R>)> {
        let ray = BkdRay(*ray);
        self.find(&ray)
            .map(|(normal, object)| (normal, object.get_material()))
    }
}

pub struct BkdRay(pub Ray3<f32>);

impl bkd_tree::Ray for BkdRay {
    type Dim = Dim3;

    fn plane_intersections(&self, min: f32, max: f32, axis: Dim3) -> Option<(f32, f32)> {
        let &BkdRay(ray) = self;

        let (origin, direction) = match axis {
            Dim3::X => (ray.origin.x, ray.direction.x),
            Dim3::Y => (ray.origin.y, ray.direction.y),
            Dim3::Z => (ray.origin.z, ray.direction.z),
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
    fn plane_distance(&self, min: f32, max: f32, axis: Dim3) -> (f32, f32) {
        let &BkdRay(ray) = self;

        let (origin, direction) = match axis {
            Dim3::X => (ray.origin.x, ray.direction.x),
            Dim3::Y => (ray.origin.y, ray.direction.y),
            Dim3::Z => (ray.origin.z, ray.direction.z),
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

fn decode_sky_color(entry: Entry<'_>) -> Result<Sky, String> {
    let fields = entry.as_object().ok_or("not an object")?;

    let color = match fields.get("color") {
        Some(v) => try_for!(tracer::decode_parametric_number(v), "color"),
        None => return Err("missing field 'color'".into()),
    };

    Ok(Sky::Color(color))
}

pub fn decode_world<F: Fn(String) -> P, P: AsRef<Path>, R: Rng + 'static>(
    entry: Entry<'_>,
    make_path: F,
) -> Result<World<R>, String> {
    let fields = entry.as_object().ok_or("not an object")?;

    let sky = match fields.get("sky") {
        Some(v) => try_for!(v.dynamic_decode(), "sky"),
        None => return Err("missing field 'sky'".into()),
    };

    let object_protos = match fields.get("objects") {
        Some(v) => try_for!(
            v.as_list().ok_or(String::from("expected a list")),
            "objects"
        ),
        None => return Err("missing field 'objects'".into()),
    };

    let mut objects: Vec<Arc<shapes::Shape<R>>> = Vec::new();
    let mut lights = Vec::new();

    for (i, object) in object_protos.into_iter().enumerate() {
        let shape = try_for!(object.dynamic_decode(), format!("objects: [{}]", i));
        match shape {
            Object::Shape { shape, emissive } => {
                let shape = Arc::new(shape);
                if emissive {
                    lights.push(Lamp::Shape(shape.clone()));
                }
                objects.push(shape);
            }
            Object::Mesh {
                file,
                mut materials,
                scale,
                transform,
            } => {
                let path = make_path(file);
                let obj = obj::Obj::load(path.as_ref()).map_err(|error| error.to_string())?;
                for object in &obj.objects {
                    println!("adding object '{}'", object.name);

                    let (object_material, emissive) = match materials.remove(&object.name) {
                        Some(m) => {
                            let (material, emissive): (
                                Box<dyn Material<R> + 'static + Send + Sync>,
                                bool,
                            ) = m;
                            (Arc::new(material), emissive)
                        }
                        None => {
                            return Err(format!(
                                "objects: [{}]: missing field '{}'",
                                i, object.name
                            ))
                        }
                    };

                    for group in &object.groups {
                        for shape in &group.polys {
                            match *shape {
                                genmesh::Polygon::PolyTri(genmesh::Triangle { x, y, z }) => {
                                    let mut triangle =
                                        make_triangle(&obj, x, y, z, object_material.clone());
                                    triangle.scale(scale);
                                    triangle.transform(transform);
                                    let triangle = Arc::new(triangle);
                                    if emissive {
                                        lights.push(Lamp::Shape(triangle.clone()));
                                    }

                                    objects.push(triangle);
                                }
                                _ => {}
                            }
                        }
                    }
                }
            }
            Object::Lamp(light) => lights.push(light),
        }
    }

    println!("the scene contains {} objects", objects.len());
    println!("building BKD-Tree... ");
    let tree = bkd_tree::BkdTree::new(objects, 10); //TODO: make arrity configurable
    println!("done building BKD-Tree");
    Ok(World {
        sky: sky,
        lights: lights,
        objects: Box::new(tree) as Box<dyn ObjectContainer<R> + 'static + Send + Sync>,
    })
}

fn vertex_to_point(v: &[f32; 3]) -> Point3<f32> {
    Point3::new(v[0], v[1], v[2])
}

fn vertex_to_vector(v: &[f32; 3]) -> Vector3<f32> {
    Vector3::new(v[0], v[1], v[2])
}

fn make_triangle<M: obj::GenPolygon, R: Rng>(
    obj: &obj::Obj<'_, M>,
    obj::IndexTuple(v1, _t1, n1): obj::IndexTuple,
    obj::IndexTuple(v2, _t2, n2): obj::IndexTuple,
    obj::IndexTuple(v3, _t3, n3): obj::IndexTuple,
    material: Arc<Box<dyn Material<R> + 'static + Send + Sync>>,
) -> shapes::Shape<R> {
    let v1 = vertex_to_point(&obj.position[v1]);
    let v2 = vertex_to_point(&obj.position[v2]);
    let v3 = vertex_to_point(&obj.position[v3]);

    let (n1, n2, n3) = match (n1, n2, n3) {
        (Some(n1), Some(n2), Some(n3)) => {
            let n1 = vertex_to_vector(&obj.normal[n1]);
            let n2 = vertex_to_vector(&obj.normal[n2]);
            let n3 = vertex_to_vector(&obj.normal[n3]);
            (n1, n2, n3)
        }
        _ => {
            let a = v2 - &v1;
            let b = v3 - &v1;
            let normal = a.cross(b).normalize();
            (normal, normal, normal)
        }
    };

    shapes::Triangle {
        v1: shapes::Vertex {
            position: v1,
            normal: n1,
        },
        v2: shapes::Vertex {
            position: v2,
            normal: n2,
        },
        v3: shapes::Vertex {
            position: v3,
            normal: n3,
        },
        material: material,
    }
}
