use std;
use std::collections::HashMap;
use std::sync::Arc;
use std::num::{FloatMath, Float};

use cgmath;
use cgmath::{Vector, EuclideanVector, Vector3};
use cgmath::{Point, Point3};
use cgmath::Intersect;
use cgmath::{Ray, Ray3};

use tracer;
use tracer::Material;

use config;
use config::{FromConfig, Type};

use bkdtree;
use materials;

pub use self::Shape::{Sphere, Plane, Triangle};
pub use self::ProxyShape::{DecodedShape, Mesh};

pub struct Vertex<S> {
    pub position: Point3<S>,
    pub normal: Vector3<S>
}

pub enum ProxyShape {
    DecodedShape { shape: Shape, emissive: bool },
    Mesh { file: String, materials: HashMap<String, config::ConfigItem> }
}

pub enum Shape {
    Sphere { position: Point3<f64>, radius: f64, material: materials::MaterialBox },
    Plane { shape: cgmath::Plane<f64>, material: materials::MaterialBox },
    Triangle { v1: Vertex<f64>, v2: Vertex<f64>, v3: Vertex<f64>, material: Arc<materials::MaterialBox> }
}

impl Shape {
    pub fn ray_intersect(&self, ray: &Ray3<f64>) -> Option<(f64, Ray3<f64>)> {
        match *self {
            Sphere {ref position, radius, ..} => {
                let sphere = cgmath::Sphere {
                    radius: radius,
                    center: position.clone()
                };
                (sphere, ray.clone())
                    .intersection()
                    .map(|intersection| (intersection.sub_p(&ray.origin).length(), Ray::new(intersection, intersection.sub_p(position).normalize())) )
            },
            Plane {ref shape, ..} => {
                (shape.clone(), ray.clone())
                    .intersection()
                    .map(|intersection| (intersection.sub_p(&ray.origin).length(), Ray::new(intersection, shape.n.clone())) )
            },
            Triangle {ref v1, ref v2, ref v3, ..} => {
                //Möller–Trumbore intersection algorithm
                let epsilon = 0.000001f64;
                let e1 = v2.position.sub_p(&v1.position);
                let e2 = v3.position.sub_p(&v1.position);

                let p = ray.direction.cross(&e2);
                let det = e1.dot(&p);

                if det > -epsilon && det < epsilon {
                    return None;
                }

                let inv_det = 1.0 / det;
                let t = ray.origin.sub_p(&v1.position);
                let u = t.dot(&p) * inv_det;

                //Outside triangle
                if u < 0.0 || u > 1.0 {
                    return None;
                }

                let q = t.cross(&e1);
                let v = ray.direction.dot(&q) * inv_det;

                //Outside triangle
                if v < 0.0 || u + v > 1.0 {
                    return None;
                }

                let dist = e2.dot(&q) * inv_det;
                if dist > epsilon {
                    let hit_position = ray.origin.add_v(&ray.direction.mul_s(dist));
                    let normal = v1.normal.mul_s(1.0 - (u + v)).add_v(&v2.normal.mul_s(u)).add_v(&v3.normal.mul_s(v));
                    Some(( dist, Ray::new(hit_position, normal.normalize()) ))
                } else {
                    None
                }
            }
        }
    }

    pub fn get_material(&self) -> &Material {
        match *self {
            Sphere { ref material, .. } => & **material,
            Plane { ref material, .. } => & **material,
            Triangle { ref material, .. } => & **material.deref()
        }
    }

    pub fn sample_point<R: std::rand::Rng>(&self, rng: &mut R) -> Option<Ray3<f64>> {
        match *self {
            Sphere { ref position, radius, .. } => {
                let u = rng.gen();
                let v = rng.gen();
                let theta = 2.0f64 * std::f64::consts::PI * u;
                let phi = (2.0f64 * v - 1.0).acos();
                let sphere_point = Vector3::new(
                    phi.sin() * theta.cos(),
                    phi.sin() * theta.sin(),
                    phi.cos()
                );

                Some(Ray::new(position.add_v(&sphere_point.mul_s(radius)), sphere_point))
            },
            Plane {..} => None,
            Triangle { ref v1, ref v2, ref v3, .. } => {
                let u: f64 = rng.gen();
                let v = rng.gen();

                let a = v2.position.sub_p(&v1.position);
                let b = v3.position.sub_p(&v1.position);

                let (u, v) = if u + v > 1.0 {
                    (1.0 - u, 1.0 - v)
                } else {
                    (u, v)
                };

                let position = v1.position.add_v(&a.mul_s(u)).add_v(&b.mul_s(v));
                let normal = v1.normal.mul_s(1.0 - (u + v)).add_v(&v2.normal.mul_s(u)).add_v(&v3.normal.mul_s(v));

                Some(Ray::new(position, normal.normalize()))
            }
        }
    }

    pub fn surface_area(&self) -> f64 {
        match *self {
            Sphere { radius, .. } => radius * radius * 4.0 * std::f64::consts::PI,
            Plane {..} => Float::infinity(),
            Triangle { ref v1, ref v2, ref v3, .. } => {
                let a = v2.position.sub_p(&v1.position);
                let b = v3.position.sub_p(&v1.position);
                0.5 * a.cross(&b).length()
            }
        }
    }
}

impl<'a> bkdtree::Element<tracer::BkdRay<'a>, Ray3<f64>> for Arc<Shape> {
    fn get_bounds_interval(&self, axis: uint) -> (f64, f64) {
        match *self.deref() {
            Sphere { ref position, radius, .. } => match axis {
                0 => (position.x - radius, position.x + radius),
                1 => (position.y - radius, position.y + radius),
                _ => (position.z - radius, position.z + radius)
            },
            Plane {shape, ..} => {
                let point = shape.n.mul_s(shape.d);
                match axis {
                    0 if shape.n.x.abs() == 1.0 => (point.x, point.x),
                    1 if shape.n.x.abs() == 1.0 => (point.y, point.y),
                    2 if shape.n.x.abs() == 1.0 => (point.z, point.z),
                    _ => (Float::neg_infinity(), Float::infinity())
                }
            },
            Triangle { ref v1, ref v2, ref v3, .. } => {
                let min = v1.position.min(&v2.position).min(&v3.position);
                let max = v1.position.max(&v2.position).max(&v3.position);

                match axis {
                    0 => (min.x, max.x),
                    1 => (min.y, max.y),
                    _ => (min.z, max.z)
                }
            }
        }
    }

    fn intersect(&self, ray: &tracer::BkdRay) -> Option<(f64, Ray3<f64>)> {
        let &tracer::BkdRay(ray) = ray;
        self.ray_intersect(ray)
    }
}



pub fn register_types(context: &mut config::ConfigContext) {
    context.insert_grouped_type("Shape", "Sphere", decode_sphere);
    context.insert_grouped_type("Shape", "Plane", decode_plane);
    context.insert_grouped_type("Shape", "Mesh", decode_mesh);
}

fn decode_sphere(context: &config::ConfigContext, items: HashMap<String, config::ConfigItem>) -> Result<ProxyShape, String> {
    let mut items = items;

    let position = match items.remove("position") {
        Some(v) => try!(context.decode_structure_of_type(&Type::single("Vector"), v), "position"),
        None => return Err(String::from_str("missing field 'position'"))
    };

    let radius = match items.remove("radius") {
        Some(v) => try!(FromConfig::from_config(v), "radius"),
        None => return Err(String::from_str("missing field 'radius'"))
    };

    let (material, emissive): (materials::MaterialBox, bool) = match items.remove("material") {
        Some(v) => try!(context.decode_structure_from_group("Material", v), "material"),
        None => return Err(String::from_str("missing field 'material'"))
    };

    Ok(DecodedShape {
        shape: Sphere {
            position: Point::from_vec(&position),
            radius: radius,
            material: material
        },
        emissive: emissive
    })
}

fn decode_plane(context: &config::ConfigContext, mut items: HashMap<String, config::ConfigItem>) -> Result<ProxyShape, String> {
    let origin = match items.remove("origin") {
        Some(v) => try!(context.decode_structure_of_type(&Type::single("Vector"), v), "origin"),
        None => return Err(String::from_str("missing field 'origin'"))
    };

    let normal = match items.remove("normal") {
        Some(v) => try!(context.decode_structure_of_type(&Type::single("Vector"), v), "normal"),
        None => return Err(String::from_str("missing field 'normal'"))
    };

    let (material, emissive): (materials::MaterialBox, bool) = match items.remove("material") {
        Some(v) => try!(context.decode_structure_from_group("Material", v), "material"),
        None => return Err(String::from_str("missing field 'material'"))
    };

    Ok(DecodedShape {
        shape: Plane {
            shape: cgmath::Plane::from_point_normal(Point::from_vec(&origin), normal),
            material: material
        },
        emissive: emissive
    })
}

fn decode_mesh(_context: &config::ConfigContext, items: HashMap<String, config::ConfigItem>) -> Result<ProxyShape, String> {
    let mut items = items;

    let file_name: String = match items.remove("file") {
        Some(v) => try!(FromConfig::from_config(v), "file"),
        None => return Err(String::from_str("missing field 'file'"))
    };

    let materials = match items.remove("materials") {
        Some(config::Structure(_, fields)) => fields,
        Some(v) => return Err(format!("materials: expected a structure, but found '{}'", v)),
        None => return Err(String::from_str("missing field 'materials'"))
    };

    Ok(Mesh{
        file: file_name,
        materials: materials
    })
}