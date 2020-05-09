use std;
use std::sync::Arc;
use std::ops::Deref;

use std::f64::{INFINITY, NEG_INFINITY};

use cgmath;
use cgmath::{Vector, EuclideanVector, Vector3};
use cgmath::{Point, Point3};
use cgmath::Intersect;
use cgmath::{Ray, Ray3};
use cgmath::{Transform, AffineMatrix3};

use tracer::{Material, ParametricValue};

use config::Prelude;
use config::entry::Entry;

use spatial::{bkd_tree, Dim3};
use materials;
use world;
use math;

use rand;

const EPSILON: f64 = 0.000000001;

mod distance_estimators;

pub use self::Shape::{ Sphere, Plane, Triangle, RayMarched };

type DistanceEstimator = Box<ParametricValue<Point3<f64>, f64>>;

pub struct Vertex<S> {
    pub position: Point3<S>,
    pub normal: Vector3<S>
}

pub enum Shape {
    Sphere { position: Point3<f64>, radius: f64, material: materials::MaterialBox },
    Plane { shape: cgmath::Plane<f64>, material: materials::MaterialBox },
    Triangle { v1: Vertex<f64>, v2: Vertex<f64>, v3: Vertex<f64>, material: Arc<materials::MaterialBox> },
    RayMarched { estimator: DistanceEstimator, bounds: BoundingVolume, material: materials::MaterialBox }
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
                let e1 = v2.position.sub_p(&v1.position);
                let e2 = v3.position.sub_p(&v1.position);

                let p = ray.direction.cross(&e2);
                let det = e1.dot(&p);

                if det > -EPSILON && det < EPSILON {
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
                if dist > EPSILON {
                    let hit_position = ray.origin.add_v(&ray.direction.mul_s(dist));
                    let normal = v1.normal.mul_s(1.0 - (u + v)).add_v(&v2.normal.mul_s(u)).add_v(&v3.normal.mul_s(v));
                    Some(( dist, Ray::new(hit_position, normal.normalize()) ))
                } else {
                    None
                }
            },
            RayMarched { ref estimator, ref bounds, .. } => bounds.intersect(ray).and_then(|(min, max)| {
                let origin = ray.origin.add_v(&-bounds.center().to_vec());
                let mut total_distance = min;
                while total_distance < max {
                    let p = origin.add_v(&ray.direction.mul_s(total_distance));
                    let distance = estimator.get(&p);
                    total_distance += distance;
                    if distance < EPSILON || total_distance > max {
                        //println!("dist: {}", distance);
                        break;
                    }
                }

                if total_distance <= max {
                    let p = origin.add_v(&ray.direction.mul_s(total_distance - EPSILON));
                    let x_dir = Vector3::new(EPSILON, 0.0, 0.0);
                    let y_dir = Vector3::new(0.0, EPSILON, 0.0);
                    let z_dir = Vector3::new(0.0, 0.0, EPSILON);
                    let n = Vector3::new(
                        estimator.get(&p.add_v(&x_dir)) - estimator.get(&p.add_v(&-x_dir)),
                        estimator.get(&p.add_v(&y_dir)) - estimator.get(&p.add_v(&-y_dir)),
                        estimator.get(&p.add_v(&z_dir)) - estimator.get(&p.add_v(&-z_dir)),
                    ).normalize();
                    let p = ray.origin.add_v(&ray.direction.mul_s(total_distance));
                    //println!("n: {:?}", n);
                    Some((total_distance, Ray3::new(p, n)))
                } else {
                    None
                }
            }),
        }
    }

    pub fn get_material(&self) -> &Material {
        match *self {
            Sphere { ref material, .. } => & **material,
            Plane { ref material, .. } => & **material,
            Triangle { ref material, .. } => & **material.deref(),
            RayMarched { ref material, .. } => & **material
        }
    }

    pub fn sample_point<R: rand::Rng>(&self, rng: &mut R) -> Option<Ray3<f64>> {
        match *self {
            Sphere { ref position, radius, .. } => {
                let sphere_point = math::utils::sample_sphere(rng);
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
            },
            RayMarched { .. } => None
        }
    }

    pub fn sample_towards<R: rand::Rng>(&self, rng: &mut R, target: &Point3<f64>) -> Option<Ray3<f64>> {
        match *self {
            Sphere { ref position, radius, .. } => {
                let dir = position.sub_p(&target);
                let dist2 = dir.length2();

                if dist2 > radius * radius {
                    let cos_theta_max = (1.0 - (radius * radius) / dist2).max(0.0).sqrt();
                    let ray_dir = math::utils::sample_cone(rng, &dir.normalize(), cos_theta_max);
                    self.ray_intersect(&Ray3::new(*target, ray_dir)).map(|(_, n)| n)
                } else {
                    self.sample_point(rng)
                }
            },
            _ => self.sample_point(rng),
        }
    }

    pub fn solid_angle_towards(&self, target: &Point3<f64>) -> Option<f64> {
        match *self {
            Sphere { ref position, radius, .. } => {
                let dist2 = position.sub_p(target).length2();
                if dist2 > radius * radius {
                    let cos_theta_max = (1.0 - (radius * radius) / dist2).max(0.0).sqrt();
                    let a = math::utils::solid_angle(cos_theta_max);
                    Some(a)
                } else {
                    None
                }
            },
            _ => None
        }
    }

    pub fn surface_area(&self) -> f64 {
        match *self {
            Sphere { radius, .. } => radius * radius * 4.0 * std::f64::consts::PI,
            Plane {..} => INFINITY,
            Triangle { ref v1, ref v2, ref v3, .. } => {
                let a = v2.position.sub_p(&v1.position);
                let b = v3.position.sub_p(&v1.position);
                0.5 * a.cross(&b).length()
            },
            RayMarched { .. } => INFINITY
        }
    }

    pub fn scale(&mut self, scale: f64) {
        match *self {
            Sphere { ref mut radius, ref mut position, .. } => {
                *radius *= scale;
                position.mul_self_s(scale);
            },
            Plane { .. } => {},
            Triangle { ref mut v1, ref mut v2, ref mut v3, .. } => {
                v1.position.mul_self_s(scale);
                v2.position.mul_self_s(scale);
                v3.position.mul_self_s(scale);
            },
            RayMarched { .. } => {}
        }
    }

    pub fn transform(&mut self, transform: &AffineMatrix3<f64>) {
        match *self {
            Sphere { ref mut position, .. } => {
                *position = transform.transform_point(position);
            },
            Plane { .. } => {},
            Triangle { ref mut v1, ref mut v2, ref mut v3, .. } => {
                v1.normal = transform.transform_vector(&v1.normal);
                v2.normal = transform.transform_vector(&v2.normal);
                v3.normal = transform.transform_vector(&v3.normal);
                v1.position = transform.transform_point(&v1.position);
                v2.position = transform.transform_point(&v2.position);
                v3.position = transform.transform_point(&v3.position);
            },
            RayMarched { .. } => {}
        }
    }
}

impl bkd_tree::Element for Arc<Shape> {
    type Item = Ray3<f64>;
    type Ray = world::BkdRay;

    fn get_bounds_interval(&self, axis: Dim3) -> (f64, f64) {
        match *self.deref() {
            Sphere { ref position, radius, .. } => match axis {
                Dim3::X => (position.x - radius, position.x + radius),
                Dim3::Y => (position.y - radius, position.y + radius),
                Dim3::Z => (position.z - radius, position.z + radius)
            },
            Plane {shape, ..} => {
                let point = shape.n.mul_s(shape.d);
                match axis {
                    Dim3::X if shape.n.x.abs() == 1.0 => (point.x, point.x),
                    Dim3::Y if shape.n.x.abs() == 1.0 => (point.y, point.y),
                    Dim3::Z if shape.n.x.abs() == 1.0 => (point.z, point.z),
                    _ => (NEG_INFINITY, INFINITY)
                }
            },
            Triangle { ref v1, ref v2, ref v3, .. } => {
                let min = v1.position.min(&v2.position).min(&v3.position);
                let max = v1.position.max(&v2.position).max(&v3.position);

                match axis {
                    Dim3::X => (min.x, max.x),
                    Dim3::Y => (min.y, max.y),
                    Dim3::Z => (min.z, max.z)
                }
            },
            RayMarched { ref bounds, .. } => match axis {
                Dim3::X => bounds.x_interval(),
                Dim3::Y => bounds.y_interval(),
                Dim3::Z => bounds.z_interval(),
            }
        }
    }

    fn intersect(&self, ray: &world::BkdRay) -> Option<(f64, Ray3<f64>)> {
        let &world::BkdRay(ref ray) = ray;
        self.ray_intersect(ray)
    }
}

pub enum BoundingVolume {
    Box(Point3<f64>, Point3<f64>),
    Sphere(Point3<f64>, f64)
}

impl BoundingVolume {
    pub fn x_interval(&self) -> (f64, f64) {
        match *self {
            BoundingVolume::Box(min, max) => (min.x, max.x),
            BoundingVolume::Sphere(center, radius) => (center.x - radius, center.x + radius),
        }
    }

    pub fn y_interval(&self) -> (f64, f64) {
        match *self {
            BoundingVolume::Box(min, max) => (min.y, max.y),
            BoundingVolume::Sphere(center, radius) => (center.y - radius, center.y + radius),
        }
    }

    pub fn z_interval(&self) -> (f64, f64) {
        match *self {
            BoundingVolume::Box(min, max) => (min.z, max.z),
            BoundingVolume::Sphere(center, radius) => (center.z - radius, center.z + radius),
        }
    }

    pub fn intersect(&self, ray: &Ray3<f64>) -> Option<(f64, f64)> {
        match *self {
            BoundingVolume::Box(min, max) => {
                let inv_dir = Vector3::new(1.0 / ray.direction.x, 1.0 / ray.direction.y, 1.0 / ray.direction.z);
                let (mut t_min, mut t_max) = if inv_dir.x < 0.0 {
                    ((max.x - ray.origin.x) * inv_dir.x, (min.x - ray.origin.x) * inv_dir.x)
                } else {
                    ((min.x - ray.origin.x) * inv_dir.x, (max.x - ray.origin.x) * inv_dir.x)
                };

                let (ty_min, ty_max) = if inv_dir.y < 0.0 {
                    ((max.y - ray.origin.y) * inv_dir.y, (min.y - ray.origin.y) * inv_dir.y)
                } else {
                    ((min.y - ray.origin.y) * inv_dir.y, (max.y - ray.origin.y) * inv_dir.y)
                };

                if t_min > ty_max || ty_min > t_max {
                    return None;
                }

                if ty_min > t_min {
                    t_min = ty_min;
                }

                if ty_max > t_max {
                    t_max = ty_max;
                }

                let (tz_min, tz_max) = if inv_dir.z < 0.0 {
                    ((max.z - ray.origin.z) * inv_dir.z, (min.z - ray.origin.z) * inv_dir.z)
                } else {
                    ((min.z - ray.origin.z) * inv_dir.z, (max.z - ray.origin.z) * inv_dir.z)
                };

                if t_min > tz_max || tz_min > t_max {
                    return None;
                }

                if tz_min > t_min {
                    t_min = tz_min;
                }

                if tz_max > t_max {
                    t_max = tz_max;
                }

                t_min = t_min.max(0.0);

                if t_min < t_max {
                    Some((t_min, t_max))
                } else {
                    None
                }
            },
            BoundingVolume::Sphere(center, radius) => {
                let l = center.sub_p(&ray.origin);
                let tca = l.dot(&ray.direction);
                if tca < 0.0 { return None; }
                let d2 = l.dot(&l) - tca*tca;
                if d2 > radius*radius { return None; }
                let thc = (radius*radius - d2).sqrt();
                Some((tca - thc, tca + thc))
            },
        }
    }

    fn center(&self) -> Point3<f64> {
        match *self {
            BoundingVolume::Box(min, max) => min.add_v(&max.to_vec()).mul_s(0.5),
            BoundingVolume::Sphere(center, _) => center
        }
    }
}


pub fn register_types(context: &mut Prelude) {
    {
        let mut group = context.object("Shape".into());
        group.object("Sphere".into()).add_decoder(decode_sphere);
        group.object("Plane".into()).add_decoder(decode_plane);
        group.object("Mesh".into()).add_decoder(decode_mesh);
        group.object("RayMarched".into()).add_decoder(decode_ray_marched);
    }
    {
        let mut group = context.object("Bounds".into());
        group.object("Box".into()).add_decoder(decode_bounding_box);
        group.object("Sphere".into()).add_decoder(decode_bounding_sphere);
    }

    distance_estimators::register_types(context);
}

fn decode_sphere(entry: Entry) -> Result<world::Object, String> {
    let items = try!(entry.as_object().ok_or("not an object".into()));

    let position = match items.get("position") {
        Some(v) => try!(v.dynamic_decode(), "position"),
        None => return Err("missing field 'position'".into())
    };

    let radius = match items.get("radius") {
        Some(v) => try!(v.decode(), "radius"),
        None => return Err("missing field 'radius'".into())
    };

    let (material, emissive): (materials::MaterialBox, bool) = match items.get("material") {
        Some(v) => try!(v.dynamic_decode(), "material"),
        None => return Err("missing field 'material'".into())
    };

    Ok(world::Object::Shape {
        shape: Sphere {
            position: Point::from_vec(&position),
            radius: radius,
            material: material
        },
        emissive: emissive
    })
}

fn decode_plane(entry: Entry) -> Result<world::Object, String> {
    let items = try!(entry.as_object().ok_or("not an object".into()));

    let origin = match items.get("origin") {
        Some(v) => try!(v.dynamic_decode(), "origin"),
        None => return Err("missing field 'origin'".into())
    };

    let normal = match items.get("normal") {
        Some(v) => try!(v.dynamic_decode(), "normal"),
        None => return Err("missing field 'normal'".into())
    };

    let (material, emissive): (materials::MaterialBox, bool) = match items.get("material") {
        Some(v) => try!(v.dynamic_decode(), "material"),
        None => return Err("missing field 'material'".into())
    };

    Ok(world::Object::Shape {
        shape: Plane {
            shape: cgmath::Plane::from_point_normal(Point::from_vec(&origin), normal),
            material: material
        },
        emissive: emissive
    })
}

fn decode_mesh(entry: Entry) -> Result<world::Object, String> {
    let items = try!(entry.as_object().ok_or("not an object".into()));

    let file_name: String = match items.get("file") {
        Some(v) => try!(v.decode(), "file"),
        None => return Err("missing field 'file'".into())
    };

    let scale = match items.get("scale") {
        Some(v) => try!(v.decode(), "scale"),
        None => 1.0
    };

    let transform = match items.get("transform") {
        Some(v) => AffineMatrix3 {
            mat: try!(v.dynamic_decode(), "transform")
        },
        None => Transform::identity()
    };

    let materials = match items.get("materials").map(|e| e.as_object()) {
        Some(Some(fields)) => try!(fields.into_iter().map(|(k, v)| {
            let i = try!(v.dynamic_decode());
            Ok((k.into(), i))
        }).collect()),
        Some(None) => return Err(format!("materials: expected a structure, but found something else")), //TODO: better handling
        None => return Err("missing field 'materials'".into())
    };

    Ok(world::Object::Mesh {
        file: file_name,
        materials: materials,
        scale: scale,
        transform: transform,
    })
}

fn decode_ray_marched(entry: Entry) -> Result<world::Object, String> {
    let items = try!(entry.as_object().ok_or("not an object".into()));

    let bounds = match items.get("bounds") {
        Some(v) => try!(v.dynamic_decode(), "bounds"),
        None => return Err("missing field 'bounds'".into())
    };

    let estimator = match items.get("shape") {
        Some(v) => try!(v.dynamic_decode(), "shape"),
        None => return Err("missing field 'shape'".into())
    };

    let (material, emissive): (materials::MaterialBox, bool) = match items.get("material") {
        Some(v) => try!(v.dynamic_decode(), "material"),
        None => return Err("missing field 'material'".into())
    };

    Ok(world::Object::Shape {
        shape: RayMarched {
            bounds: bounds,
            estimator: estimator,
            material: material
        },
        emissive: emissive
    })
}

fn decode_bounding_sphere(entry: Entry) -> Result<BoundingVolume, String> {
    let items = try!(entry.as_object().ok_or("not an object".into()));

    let position = match items.get("position") {
        Some(v) => try!(v.dynamic_decode(), "position"),
        None => return Err("missing field 'position'".into())
    };

    let radius = match items.get("radius") {
        Some(v) => try!(v.decode(), "radius"),
        None => return Err("missing field 'radius'".into())
    };

    Ok(BoundingVolume::Sphere(position, radius))
}

fn decode_bounding_box(entry: Entry) -> Result<BoundingVolume, String> {
    let items = try!(entry.as_object().ok_or("not an object".into()));

    let min = match items.get("min") {
        Some(v) => try!(v.dynamic_decode(), "min"),
        None => return Err("missing field 'min'".into())
    };

    let max = match items.get("max") {
        Some(v) => try!(v.dynamic_decode(), "max"),
        None => return Err("missing field 'max'".into())
    };

    Ok(BoundingVolume::Box(min, max))
}
