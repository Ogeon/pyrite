use std;
use std::ops::Deref;
use std::sync::Arc;

use std::f32::{INFINITY, NEG_INFINITY};

use cgmath::{
    ElementWise, EuclideanSpace, InnerSpace, Matrix3, Matrix4, Point2, Point3, SquareMatrix,
    Transform, Vector3,
};
use collision::{Continuous, Ray3};

use rand::Rng;

use crate::tracer::ParametricValue;

use crate::materials::Material;
use crate::math::{self, DIST_EPSILON};
use crate::spatial::{bkd_tree, Dim3};
use crate::world;

pub use self::Shape::{Plane, RayMarched, Sphere, Triangle};

const EPSILON: f32 = DIST_EPSILON;

pub mod distance_estimators;

type DistanceEstimator = Box<dyn ParametricValue<Point3<f32>, f32>>;

pub struct Vertex<S> {
    pub position: Point3<S>,
    pub normal: Vector3<S>,
    pub texture: Point2<S>,
}

pub enum Shape {
    Sphere {
        position: Point3<f32>,
        radius: f32,
        material: Material,
    },
    Plane {
        shape: collision::Plane<f32>,
        tangent: Vector3<f32>,
        binormal: Vector3<f32>,
        texture_scale: f32,
        material: Material,
    },
    Triangle {
        v1: Vertex<f32>,
        v2: Vertex<f32>,
        v3: Vertex<f32>,
        material: Arc<Material>,
    },
    RayMarched {
        estimator: DistanceEstimator,
        bounds: BoundingVolume,
        material: Material,
    },
}

impl Shape {
    pub fn ray_intersect(&self, ray: &Ray3<f32>) -> Option<Intersection> {
        match *self {
            Sphere {
                ref position,
                radius,
                ..
            } => {
                let sphere = collision::Sphere {
                    radius,
                    center: position.clone(),
                };
                sphere.intersection(ray).map(|intersection| Intersection {
                    distance: (intersection - ray.origin).magnitude(),
                    position: intersection,
                    normal: (intersection - position).normalize(),
                    texture: Point2::origin(),
                })
            }
            Plane {
                ref shape,
                tangent,
                binormal,
                texture_scale,
                ..
            } => shape.intersection(ray).map(|intersection| {
                let Vector3 {
                    x: tex_x, y: tex_y, ..
                } = Matrix3::from_cols(tangent, binormal, shape.n)
                    .invert()
                    .expect("could not invert plane texture space")
                    * intersection.to_vec();

                Intersection {
                    distance: (intersection - ray.origin).magnitude(),
                    position: intersection,
                    normal: shape.n,
                    texture: Point2::new(tex_x, tex_y) / texture_scale,
                }
            }),
            Triangle {
                ref v1,
                ref v2,
                ref v3,
                ..
            } => {
                //Möller–Trumbore intersection algorithm
                let e1 = v2.position - v1.position;
                let e2 = v3.position - v1.position;

                let p = ray.direction.cross(e2);
                let det = e1.dot(p);

                if det > -EPSILON && det < EPSILON {
                    return None;
                }

                let inv_det = 1.0 / det;
                let t = ray.origin - v1.position;
                let u = t.dot(p) * inv_det;

                //Outside triangle
                if u < 0.0 || u > 1.0 {
                    return None;
                }

                let q = t.cross(e1);
                let v = ray.direction.dot(q) * inv_det;

                //Outside triangle
                if v < 0.0 || u + v > 1.0 {
                    return None;
                }

                let dist = e2.dot(q) * inv_det;
                if dist > EPSILON {
                    let hit_position = ray.origin + ray.direction * dist;
                    let normal = v1.normal * (1.0 - (u + v)) + v2.normal * u + v3.normal * v;
                    let texture = (v1.texture * (1.0 - (u + v)))
                        .add_element_wise(v2.texture * u)
                        .add_element_wise(v3.texture * v);
                    Some(Intersection {
                        distance: dist,
                        position: hit_position,
                        normal: normal.normalize(),
                        texture,
                    })
                } else {
                    None
                }
            }
            RayMarched {
                ref estimator,
                ref bounds,
                ..
            } => bounds.intersect(ray).and_then(|(min, max)| {
                let origin = ray.origin + -bounds.center().to_vec();
                let mut total_distance = min;
                while total_distance < max {
                    let p = origin + ray.direction * total_distance;
                    let distance = estimator.get(&p);
                    total_distance += distance;
                    if distance < EPSILON || total_distance > max {
                        //println!("dist: {}", distance);
                        break;
                    }
                }

                if total_distance <= max {
                    let p = origin + ray.direction * (total_distance - EPSILON);
                    let x_dir = Vector3::new(EPSILON, 0.0, 0.0);
                    let y_dir = Vector3::new(0.0, EPSILON, 0.0);
                    let z_dir = Vector3::new(0.0, 0.0, EPSILON);
                    let n = Vector3::new(
                        estimator.get(&(p + x_dir)) - estimator.get(&(p + -x_dir)),
                        estimator.get(&(p + y_dir)) - estimator.get(&(p + -y_dir)),
                        estimator.get(&(p + z_dir)) - estimator.get(&(p + -z_dir)),
                    )
                    .normalize();
                    let p = ray.origin + ray.direction * total_distance;

                    Some(Intersection {
                        distance: total_distance,
                        position: p,
                        normal: n,
                        texture: Point2::origin(),
                    })
                } else {
                    None
                }
            }),
        }
    }

    pub fn get_material(&self) -> &Material {
        match *self {
            Sphere { ref material, .. } => material,
            Plane { ref material, .. } => material,
            Triangle { ref material, .. } => &**material,
            RayMarched { ref material, .. } => material,
        }
    }

    pub fn sample_point(&self, rng: &mut impl Rng) -> Option<(Ray3<f32>, Point2<f32>)> {
        match *self {
            Sphere {
                ref position,
                radius,
                ..
            } => {
                let sphere_point = math::utils::sample_sphere(rng);
                Some((
                    Ray3::new(position + sphere_point * radius, sphere_point),
                    Point2::origin(),
                ))
            }
            Plane { .. } => None,
            Triangle {
                ref v1,
                ref v2,
                ref v3,
                ..
            } => {
                let u: f32 = rng.gen();
                let v = rng.gen();

                let a = v2.position - v1.position;
                let b = v3.position - v1.position;

                let (u, v) = if u + v > 1.0 {
                    (1.0 - u, 1.0 - v)
                } else {
                    (u, v)
                };

                let position = v1.position + a * u + b * v;
                let normal = v1.normal * (1.0 - (u + v)) + v2.normal * u + v3.normal * v;
                let texture = (v1.texture * (1.0 - (u + v)))
                    .add_element_wise(v2.texture * u)
                    .add_element_wise(v3.texture * v);

                Some((Ray3::new(position, normal.normalize()), texture))
            }
            RayMarched { .. } => None,
        }
    }

    pub fn sample_towards(
        &self,
        rng: &mut impl Rng,
        target: &Point3<f32>,
    ) -> Option<(Ray3<f32>, Point2<f32>)> {
        match *self {
            Sphere {
                ref position,
                radius,
                ..
            } => {
                let radius = (radius - DIST_EPSILON).max(0.0);
                let dir = position - target;
                let dist2 = dir.magnitude2();

                if dist2 > radius * radius {
                    let cos_theta_max = (1.0 - (radius * radius) / dist2).max(0.0).sqrt();

                    let ray_dir = math::utils::sample_cone(rng, dir.normalize(), cos_theta_max);

                    let intersection = self.ray_intersect(&Ray3::new(*target, ray_dir)).map(
                        |Intersection {
                             position, normal, ..
                         }| Ray3::new(position, normal),
                    );

                    if let Some(intersection) = intersection {
                        Some((intersection, Point2::origin()))
                    } else {
                        // cheat
                        Some((Ray3::new(*target, -dir.normalize()), Point2::origin()))
                    }
                } else {
                    self.sample_point(rng)
                }
            }
            _ => self.sample_point(rng),
        }
    }

    pub fn solid_angle_towards(&self, target: &Point3<f32>) -> Option<f32> {
        match *self {
            Sphere {
                ref position,
                radius,
                ..
            } => {
                let dist2 = (position - target).magnitude2();
                if dist2 > radius * radius {
                    let cos_theta_max = (1.0 - (radius * radius) / dist2).max(0.0).sqrt();
                    let a = math::utils::solid_angle(cos_theta_max);
                    Some(a)
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    pub fn surface_area(&self) -> f32 {
        match *self {
            Sphere { radius, .. } => radius * radius * 4.0 * std::f32::consts::PI,
            Plane { .. } => INFINITY,
            Triangle {
                ref v1,
                ref v2,
                ref v3,
                ..
            } => {
                let a = v2.position - v1.position;
                let b = v3.position - v1.position;
                0.5 * a.cross(b).magnitude()
            }
            RayMarched { .. } => INFINITY,
        }
    }

    pub fn scale(&mut self, scale: f32) {
        match *self {
            Sphere {
                ref mut radius,
                ref mut position,
                ..
            } => {
                *radius *= scale;
                *position *= scale;
            }
            Plane { .. } => {}
            Triangle {
                ref mut v1,
                ref mut v2,
                ref mut v3,
                ..
            } => {
                v1.position *= scale;
                v2.position *= scale;
                v3.position *= scale;
            }
            RayMarched { .. } => {}
        }
    }

    pub fn transform(&mut self, transform: Matrix4<f32>) {
        match *self {
            Sphere {
                ref mut position, ..
            } => {
                *position = transform.transform_point(*position);
            }
            Plane { .. } => {}
            Triangle {
                ref mut v1,
                ref mut v2,
                ref mut v3,
                ..
            } => {
                v1.normal = transform.transform_vector(v1.normal);
                v2.normal = transform.transform_vector(v2.normal);
                v3.normal = transform.transform_vector(v3.normal);
                v1.position = transform.transform_point(v1.position);
                v2.position = transform.transform_point(v2.position);
                v3.position = transform.transform_point(v3.position);
            }
            RayMarched { .. } => {}
        }
    }
}

impl bkd_tree::Element for Arc<Shape> {
    type Item = Intersection;
    type Ray = world::BkdRay;

    fn get_bounds_interval(&self, axis: Dim3) -> (f32, f32) {
        match *self.deref() {
            Sphere {
                ref position,
                radius,
                ..
            } => match axis {
                Dim3::X => (position.x - radius, position.x + radius),
                Dim3::Y => (position.y - radius, position.y + radius),
                Dim3::Z => (position.z - radius, position.z + radius),
            },
            Plane { shape, .. } => {
                let point = shape.n * shape.d;
                match axis {
                    Dim3::X if shape.n.x.abs() == 1.0 => (point.x, point.x),
                    Dim3::Y if shape.n.x.abs() == 1.0 => (point.y, point.y),
                    Dim3::Z if shape.n.x.abs() == 1.0 => (point.z, point.z),
                    _ => (NEG_INFINITY, INFINITY),
                }
            }
            Triangle {
                ref v1,
                ref v2,
                ref v3,
                ..
            } => {
                let p1 = v1.position;
                let p2 = v2.position;
                let p3 = v3.position;

                match axis {
                    Dim3::X => (p1.x.min(p2.x).min(p3.x), p1.x.max(p2.x).max(p3.x)),
                    Dim3::Y => (p1.y.min(p2.y).min(p3.y), p1.y.max(p2.y).max(p3.y)),
                    Dim3::Z => (p1.z.min(p2.z).min(p3.z), p1.z.max(p2.z).max(p3.z)),
                }
            }
            RayMarched { ref bounds, .. } => match axis {
                Dim3::X => bounds.x_interval(),
                Dim3::Y => bounds.y_interval(),
                Dim3::Z => bounds.z_interval(),
            },
        }
    }

    fn intersect(&self, ray: &world::BkdRay) -> Option<(f32, Intersection)> {
        let &world::BkdRay(ref ray) = ray;
        self.ray_intersect(ray)
            .map(|intersection| (intersection.distance, intersection))
    }
}

pub struct Intersection {
    pub distance: f32,
    pub position: Point3<f32>,
    pub normal: Vector3<f32>,
    pub texture: Point2<f32>,
}

pub enum BoundingVolume {
    Box(Point3<f32>, Point3<f32>),
    Sphere(Point3<f32>, f32),
}

impl BoundingVolume {
    pub fn x_interval(&self) -> (f32, f32) {
        match *self {
            BoundingVolume::Box(min, max) => (min.x, max.x),
            BoundingVolume::Sphere(center, radius) => (center.x - radius, center.x + radius),
        }
    }

    pub fn y_interval(&self) -> (f32, f32) {
        match *self {
            BoundingVolume::Box(min, max) => (min.y, max.y),
            BoundingVolume::Sphere(center, radius) => (center.y - radius, center.y + radius),
        }
    }

    pub fn z_interval(&self) -> (f32, f32) {
        match *self {
            BoundingVolume::Box(min, max) => (min.z, max.z),
            BoundingVolume::Sphere(center, radius) => (center.z - radius, center.z + radius),
        }
    }

    pub fn intersect(&self, ray: &Ray3<f32>) -> Option<(f32, f32)> {
        match *self {
            BoundingVolume::Box(min, max) => {
                let inv_dir = Vector3::new(
                    1.0 / ray.direction.x,
                    1.0 / ray.direction.y,
                    1.0 / ray.direction.z,
                );
                let (mut t_min, mut t_max) = if inv_dir.x < 0.0 {
                    (
                        (max.x - ray.origin.x) * inv_dir.x,
                        (min.x - ray.origin.x) * inv_dir.x,
                    )
                } else {
                    (
                        (min.x - ray.origin.x) * inv_dir.x,
                        (max.x - ray.origin.x) * inv_dir.x,
                    )
                };

                let (ty_min, ty_max) = if inv_dir.y < 0.0 {
                    (
                        (max.y - ray.origin.y) * inv_dir.y,
                        (min.y - ray.origin.y) * inv_dir.y,
                    )
                } else {
                    (
                        (min.y - ray.origin.y) * inv_dir.y,
                        (max.y - ray.origin.y) * inv_dir.y,
                    )
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
                    (
                        (max.z - ray.origin.z) * inv_dir.z,
                        (min.z - ray.origin.z) * inv_dir.z,
                    )
                } else {
                    (
                        (min.z - ray.origin.z) * inv_dir.z,
                        (max.z - ray.origin.z) * inv_dir.z,
                    )
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
            }
            BoundingVolume::Sphere(center, radius) => {
                let l = center - ray.origin;
                let tca = l.dot(ray.direction);
                if tca < 0.0 {
                    return None;
                }
                let d2 = l.dot(l) - tca * tca;
                if d2 > radius * radius {
                    return None;
                }
                let thc = (radius * radius - d2).sqrt();
                Some((tca - thc, tca + thc))
            }
        }
    }

    fn center(&self) -> Point3<f32> {
        match *self {
            BoundingVolume::Box(min, max) => (min + max.to_vec()) * 0.5,
            BoundingVolume::Sphere(center, _) => center,
        }
    }
}
