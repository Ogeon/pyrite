use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use rand::Rng;

use genmesh;
use obj;

use cgmath::{EuclideanSpace, InnerSpace, Matrix4, Point2, Vector3};
use collision::Ray3;

use crate::config::entry::Entry;
use crate::config::Prelude;

use crate::spatial::{bkd_tree, Dim3};

use crate::color::{decode_color, Color};
use crate::lamp::Lamp;
use crate::materials::Material;
use crate::{
    math::RenderMath,
    shapes::{Intersection, Shape, Triangle, Vertex},
};

pub enum Sky {
    Color(RenderMath<Color>),
}

impl Sky {
    pub fn color<'a>(&'a self, _direction: &Vector3<f32>) -> &'a RenderMath<Color> {
        match self {
            Sky::Color(color) => color,
        }
    }
}

pub struct World {
    pub sky: Sky,
    pub lights: Vec<Lamp>,
    pub objects: Box<dyn ObjectContainer + 'static + Send + Sync>,
}

impl World {
    pub fn intersect(&self, ray: &Ray3<f32>) -> Option<(Intersection, &Material)> {
        self.objects.intersect(ray)
    }

    pub fn pick_lamp(&self, rng: &mut impl Rng) -> Option<(&Lamp, f32)> {
        self.lights
            .get(rng.gen_range(0, self.lights.len()))
            .map(|l| (l, 1.0 / self.lights.len() as f32))
    }
}

pub enum Object {
    Lamp(Lamp),
    Shape {
        shape: Shape,
        emissive: bool,
    },
    Mesh {
        file: String,
        materials: HashMap<String, (Material, bool)>,
        scale: f32,
        transform: Matrix4<f32>,
    },
}

pub trait ObjectContainer {
    fn intersect(&self, ray: &Ray3<f32>) -> Option<(Intersection, &Material)>;
}

impl ObjectContainer for bkd_tree::BkdTree<Arc<Shape>> {
    fn intersect(&self, ray: &Ray3<f32>) -> Option<(Intersection, &Material)> {
        let ray = BkdRay(*ray);
        self.find(&ray)
            .map(|(intersection, object)| (intersection, object.get_material()))
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

fn decode_sky_color(path: &'_ Path, entry: Entry<'_>) -> Result<Sky, String> {
    let fields = entry.as_object().ok_or("not an object")?;

    let color = match fields.get("color") {
        Some(v) => try_for!(decode_color(path, v), "color"),
        None => return Err("missing field 'color'".into()),
    };

    Ok(Sky::Color(color))
}

pub fn decode_world<F: Fn(String) -> P, P: AsRef<Path>>(
    entry: Entry<'_>,
    make_path: F,
) -> Result<World, String> {
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

    let mut objects: Vec<Arc<Shape>> = Vec::new();
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
                            let (material, emissive): (Material, bool) = m;
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
        objects: Box::new(tree) as Box<dyn ObjectContainer + 'static + Send + Sync>,
    })
}

fn make_triangle<M: obj::GenPolygon>(
    obj: &obj::Obj<'_, M>,
    obj::IndexTuple(v1, t1, n1): obj::IndexTuple,
    obj::IndexTuple(v2, t2, n2): obj::IndexTuple,
    obj::IndexTuple(v3, t3, n3): obj::IndexTuple,
    material: Arc<Material>,
) -> Shape {
    let v1 = obj.position[v1].into();
    let v2 = obj.position[v2].into();
    let v3 = obj.position[v3].into();

    let (n1, n2, n3) = match (n1, n2, n3) {
        (Some(n1), Some(n2), Some(n3)) => {
            let n1 = obj.normal[n1].into();
            let n2 = obj.normal[n2].into();
            let n3 = obj.normal[n3].into();
            (n1, n2, n3)
        }
        _ => {
            let a: Vector3<_> = v2 - v1;
            let b = v3 - v1;
            let normal = a.cross(b).normalize();
            (normal, normal, normal)
        }
    };

    let t1 = t1
        .map(|t1| obj.texture[t1].into())
        .unwrap_or(Point2::origin());
    let t2 = t2
        .map(|t2| obj.texture[t2].into())
        .unwrap_or(Point2::origin());
    let t3 = t3
        .map(|t3| obj.texture[t3].into())
        .unwrap_or(Point2::origin());

    Triangle {
        v1: Vertex {
            position: v1,
            normal: n1,
            texture: t1,
        },
        v2: Vertex {
            position: v2,
            normal: n2,
            texture: t2,
        },
        v3: Vertex {
            position: v3,
            normal: n3,
            texture: t3,
        },
        material,
    }
}
