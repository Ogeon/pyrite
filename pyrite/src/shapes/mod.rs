use std::f32::INFINITY;

use cgmath::{
    ElementWise, EuclideanSpace, InnerSpace, Matrix3, Matrix4, Point2, Point3, Quaternion, Rad,
    Transform, Vector2, Vector3,
};
use collision::{Aabb, Aabb3, Continuous, Ray3};

use crate::{
    materials::Material,
    math::{self, DIST_EPSILON},
    renderer::samplers::Sampler,
    spatial::bvh::Bounded,
    tracer::ParametricValue,
};

pub(crate) use self::Shape::{RayMarched, Sphere, Triangle};

const EPSILON: f32 = DIST_EPSILON;

pub mod distance_estimators;

type DistanceEstimator = Box<dyn ParametricValue<Point3<f32>, f32>>;

pub struct Vertex {
    pub position: Point3<f32>,
    pub normal: Normal,
    pub texture: Point2<f32>,
}

pub(crate) enum Shape<'p> {
    Sphere {
        position: Point3<f32>,
        radius: f32,
        texture_scale: Vector2<f32>,
        material: Material<'p>,
    },
    Triangle {
        v1: Vertex,
        v2: Vertex,
        v3: Vertex,
        edge1: Vector3<f32>,
        edge2: Vector3<f32>,
        material: Material<'p>,
    },
    RayMarched {
        estimator: DistanceEstimator,
        bounds: BoundingVolume,
        material: Material<'p>,
    },
}

impl<'p> Shape<'p> {
    pub fn ray_intersect(&self, ray: Ray3<f32>) -> Option<Intersection> {
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

                sphere.intersection(&ray).map(|intersection| Intersection {
                    distance: (intersection - ray.origin).magnitude(),
                    surface_point: SurfacePoint {
                        position: intersection,
                        shape: ShapeSurfacePoint::Sphere { shape: self },
                    },
                })
            }
            Triangle {
                ref v1,
                edge1: e1,
                edge2: e2,
                ..
            } => {
                //Möller–Trumbore intersection algorithm
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
                    Some(Intersection {
                        distance: dist,
                        surface_point: SurfacePoint {
                            position: hit_position,
                            shape: ShapeSurfacePoint::Triangle { shape: self, u, v },
                        },
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
                    let offset_position = origin + ray.direction * (total_distance - EPSILON);
                    let p = ray.origin + ray.direction * total_distance;

                    Some(Intersection {
                        distance: total_distance,
                        surface_point: SurfacePoint {
                            position: p,
                            shape: ShapeSurfacePoint::RayMarched {
                                shape: self,
                                offset_position,
                            },
                        },
                    })
                } else {
                    None
                }
            }),
        }
    }

    pub fn get_material(&self) -> Material {
        match *self {
            Sphere { material, .. } => material,
            Triangle { material, .. } => material,
            RayMarched { material, .. } => material,
        }
    }

    pub fn sample_point(&self, sampler: &mut dyn Sampler) -> Option<SurfacePoint> {
        match *self {
            Sphere {
                ref position,
                radius,
                ..
            } => {
                let sphere_point = math::utils::sample_sphere(sampler);

                Some(SurfacePoint {
                    position: position + sphere_point * radius,
                    shape: ShapeSurfacePoint::Sphere { shape: self },
                })
            }
            Triangle {
                ref v1,
                ref v2,
                ref v3,
                ..
            } => {
                let u: f32 = sampler.gen();
                let v = sampler.gen();

                let a = v2.position - v1.position;
                let b = v3.position - v1.position;

                let (u, v) = if u + v > 1.0 {
                    (1.0 - u, 1.0 - v)
                } else {
                    (u, v)
                };

                let position = v1.position + a * u + b * v;

                Some(SurfacePoint {
                    position,
                    shape: ShapeSurfacePoint::Triangle { shape: self, u, v },
                })
            }
            RayMarched { .. } => None,
        }
    }

    pub fn sample_towards(
        &self,
        sampler: &mut dyn Sampler,
        target: &Point3<f32>,
    ) -> Option<Intersection> {
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

                    let ray_dir = math::utils::sample_cone(sampler, dir.normalize(), cos_theta_max);

                    let intersection = self.ray_intersect(Ray3::new(*target, ray_dir));

                    if let Some(intersection) = intersection {
                        Some(intersection)
                    } else {
                        // cheat
                        Some(Intersection {
                            distance: 0.0,
                            surface_point: SurfacePoint {
                                position: *target,
                                shape: ShapeSurfacePoint::Sphere { shape: self },
                            },
                        })
                    }
                } else {
                    self.sample_point(sampler)
                        .map(|surface_point| Intersection {
                            distance: (surface_point.position - target).magnitude(),
                            surface_point,
                        })
                }
            }
            _ => self
                .sample_point(sampler)
                .map(|surface_point| Intersection {
                    distance: (surface_point.position - target).magnitude(),
                    surface_point,
                }),
        }
    }

    pub fn surface_area(&self) -> f32 {
        match *self {
            Sphere { radius, .. } => radius * radius * 4.0 * std::f32::consts::PI,
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

    pub fn emission_pdf(
        &self,
        target: Point3<f32>,
        in_direction: Vector3<f32>,
        normal: Vector3<f32>,
    ) -> f32 {
        let ray = Ray3::new(target, in_direction);
        let intersection = if let Some(intersection) = self.ray_intersect(ray) {
            intersection
        } else {
            return 0.0;
        };

        match *self {
            Sphere {
                position, radius, ..
            } => {
                let dist2 = (position - target).magnitude2();
                if dist2 > radius * radius {
                    let cos_theta_max = (1.0 - (radius * radius) / dist2).max(0.0).sqrt();
                    if cos_theta_max < 1.0 {
                        1.0 / (2.0 * std::f32::consts::PI * (1.0 - cos_theta_max))
                    } else {
                        0.0
                    }
                } else {
                    intersection.distance * intersection.distance
                        / (normal.dot(-in_direction).abs() * self.surface_area())
                }
            }
            _ => {
                intersection.distance * intersection.distance
                    / (normal.dot(-in_direction).abs() * self.surface_area())
            }
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
            Triangle {
                ref mut v1,
                ref mut v2,
                ref mut v3,
                ref mut edge1,
                ref mut edge2,
                material: _,
            } => {
                v1.position *= scale;
                v2.position *= scale;
                v3.position *= scale;
                *edge1 = v2.position - v1.position;
                *edge2 = v3.position - v1.position;
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
            Triangle {
                ref mut v1,
                ref mut v2,
                ref mut v3,
                ref mut edge1,
                ref mut edge2,
                material: _,
            } => {
                v1.normal = v1.normal.transform(transform);
                v2.normal = v2.normal.transform(transform);
                v3.normal = v3.normal.transform(transform);
                v1.position = transform.transform_point(v1.position);
                v2.position = transform.transform_point(v2.position);
                v3.position = transform.transform_point(v3.position);
                *edge1 = v2.position - v1.position;
                *edge2 = v3.position - v1.position;
            }
            RayMarched { .. } => {}
        }
    }

    fn get_sphere_surface_data(&self, surface_position: Point3<f32>) -> SurfaceData {
        if let &Sphere {
            position,
            texture_scale,
            ..
        } = self
        {
            let normal = (surface_position - position).normalize();
            let latitude = normal.y.acos();
            let longitude = normal.x.atan2(normal.z);

            let rotation = Matrix3::from_angle_y(Rad(longitude))
                * Matrix3::from_angle_x(Rad(latitude - std::f32::consts::PI * 0.5));

            let texture_coordinates = Vector2::new(
                longitude * std::f32::consts::FRAC_1_PI * 0.5,
                1.0 - (latitude * std::f32::consts::FRAC_1_PI),
            );

            SurfaceData {
                normal: Normal::new(normal, rotation.into()),
                texture: Point2::from_vec(texture_coordinates.div_element_wise(texture_scale)),
            }
        } else {
            panic!("cannot get sphere surface data from another type of shape");
        }
    }

    fn get_triangle_surface_data(&self, u: f32, v: f32) -> SurfaceData {
        if let Triangle { v1, v2, v3, .. } = self {
            let normal = Normal::on_triangle(v1.normal, v2.normal, v3.normal, u, v);
            let texture = (v1.texture * (1.0 - (u + v)))
                .add_element_wise(v2.texture * u)
                .add_element_wise(v3.texture * v);

            SurfaceData { normal, texture }
        } else {
            panic!("cannot get triangle surface data from another type of shape");
        }
    }

    fn get_ray_marched_surface_data(&self, p: Point3<f32>) -> SurfaceData {
        if let RayMarched { estimator, .. } = self {
            let x_dir = Vector3::new(EPSILON, 0.0, 0.0);
            let y_dir = Vector3::new(0.0, EPSILON, 0.0);
            let z_dir = Vector3::new(0.0, 0.0, EPSILON);
            let n = Vector3::new(
                estimator.get(&(p + x_dir)) - estimator.get(&(p + -x_dir)),
                estimator.get(&(p + y_dir)) - estimator.get(&(p + -y_dir)),
                estimator.get(&(p + z_dir)) - estimator.get(&(p + -z_dir)),
            )
            .normalize();
            SurfaceData {
                normal: Normal::from_vector(n),
                texture: Point2::origin(),
            }
        } else {
            panic!("cannot get triangle surface data from another type of shape");
        }
    }
}

impl<'p> Bounded for Shape<'p> {
    fn aabb(&self) -> Aabb3<f32> {
        match *self {
            Sphere {
                position, radius, ..
            } => Aabb3::new(
                position.sub_element_wise(radius),
                position.add_element_wise(radius),
            ),
            Triangle {
                ref v1,
                ref v2,
                ref v3,
                ..
            } => {
                let p1 = v1.position;
                let p2 = v2.position;
                let p3 = v3.position;

                Aabb3::new(p1, p2).grow(p3)
            }
            RayMarched { ref bounds, .. } => bounds.aabb(),
        }
    }
}

pub(crate) struct Plane<'p> {
    pub shape: collision::Plane<f32>,
    pub normal: Normal,
    pub texture_scale: Vector2<f32>,
    pub material: Material<'p>,
}

impl<'p> Plane<'p> {
    pub fn ray_intersect(&self, ray: &Ray3<f32>) -> Option<Intersection> {
        let Plane { ref shape, .. } = self;

        shape.intersection(ray).map(|intersection| Intersection {
            distance: (intersection - ray.origin).magnitude(),
            surface_point: SurfacePoint {
                position: intersection,
                shape: ShapeSurfacePoint::Plane { shape: self },
            },
        })
    }

    fn get_surface_data(&self, position: Point3<f32>) -> SurfaceData {
        let &Plane {
            normal,
            texture_scale,
            ..
        } = self;

        let world_space = position.to_vec();
        let normal_space = normal.into_space(world_space);

        let texture_coordinates = normal_space.truncate();
        SurfaceData {
            normal,
            texture: Point2::from_vec(texture_coordinates.div_element_wise(texture_scale)),
        }
    }
}

#[derive(Copy, Clone)]
pub(crate) struct Intersection<'a> {
    pub distance: f32,
    pub surface_point: SurfacePoint<'a>,
}

#[derive(Copy, Clone)]
pub(crate) struct SurfacePoint<'a> {
    pub position: Point3<f32>,
    pub shape: ShapeSurfacePoint<'a>,
}

impl<'a> SurfacePoint<'a> {
    pub fn get_surface_data(&self) -> SurfaceData {
        match self.shape {
            ShapeSurfacePoint::Sphere { shape } => shape.get_sphere_surface_data(self.position),
            ShapeSurfacePoint::Plane { shape } => shape.get_surface_data(self.position),
            ShapeSurfacePoint::Triangle { shape, u, v } => shape.get_triangle_surface_data(u, v),
            ShapeSurfacePoint::RayMarched {
                shape,
                offset_position,
            } => shape.get_ray_marched_surface_data(offset_position),
        }
    }

    pub fn get_material(&self) -> Material<'a> {
        match self.shape {
            ShapeSurfacePoint::Sphere { shape } => shape.get_material(),
            ShapeSurfacePoint::Plane { shape } => shape.material,
            ShapeSurfacePoint::Triangle { shape, .. } => shape.get_material(),
            ShapeSurfacePoint::RayMarched { shape, .. } => shape.get_material(),
        }
    }

    pub fn is_shape(&self, other_shape: &'a Shape<'a>) -> bool {
        match self.shape {
            ShapeSurfacePoint::Sphere { shape }
            | ShapeSurfacePoint::Triangle { shape, .. }
            | ShapeSurfacePoint::RayMarched { shape, .. } => std::ptr::eq(shape, other_shape),
            ShapeSurfacePoint::Plane { .. } => false,
        }
    }
}

#[derive(Copy, Clone)]
pub(crate) enum ShapeSurfacePoint<'a> {
    Sphere {
        shape: &'a Shape<'a>,
    },
    Plane {
        shape: &'a Plane<'a>,
    },
    Triangle {
        shape: &'a Shape<'a>,
        u: f32,
        v: f32,
    },
    RayMarched {
        shape: &'a Shape<'a>,
        offset_position: Point3<f32>,
    },
}

pub(crate) struct SurfaceData {
    pub normal: Normal,
    pub texture: Point2<f32>,
}

#[derive(Copy, Clone)]
pub struct Normal {
    vector: Vector3<f32>,
    from_space: Quaternion<f32>,
}

impl Normal {
    pub fn new(vector: Vector3<f32>, from_space: Quaternion<f32>) -> Self {
        Normal {
            vector: vector.normalize(),
            from_space: from_space.normalize(),
        }
    }

    pub fn from_vector(vector: Vector3<f32>) -> Self {
        let (x, y) = crate::math::utils::basis(vector);
        Normal {
            vector,
            from_space: Matrix3::from_cols(x, y, vector).into(),
        }
    }

    pub fn on_triangle(n1: Normal, n2: Normal, n3: Normal, u: f32, v: f32) -> Self {
        let vector = n1.vector * (1.0 - (u + v)) + n2.vector * u + n3.vector * v;
        let from_space = n1.from_space * (1.0 - (u + v)) + n2.from_space * u + n3.from_space * v;

        Normal {
            vector: vector.normalize(),
            from_space: from_space.normalize(),
        }
    }

    pub fn vector(&self) -> Vector3<f32> {
        self.vector
    }

    pub fn from_space(&self, vector: Vector3<f32>) -> Vector3<f32> {
        self.from_space * vector
    }

    pub fn into_space(&self, vector: Vector3<f32>) -> Vector3<f32> {
        self.from_space.conjugate() * vector
    }

    pub fn transform(&self, transform: Matrix4<f32>) -> Self {
        let vector = transform.transform_vector(self.vector).normalize();
        let x = transform
            .transform_vector(self.from_space(Vector3::unit_x()))
            .normalize();
        let y = transform
            .transform_vector(self.from_space(Vector3::unit_y()))
            .normalize();
        let from_space = Matrix3::from_cols(x, y, vector).into();

        Normal { vector, from_space }
    }

    pub fn tilted(&self, new_vector: Vector3<f32>) -> Normal {
        Normal {
            vector: new_vector.normalize(),
            from_space: Quaternion::from_arc(self.vector, new_vector, None).normalize()
                * self.from_space,
        }
    }
}

pub enum BoundingVolume {
    Box(Point3<f32>, Point3<f32>),
    Sphere(Point3<f32>, f32),
}

impl BoundingVolume {
    pub fn intersect(&self, ray: Ray3<f32>) -> Option<(f32, f32)> {
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

impl Bounded for BoundingVolume {
    fn aabb(&self) -> Aabb3<f32> {
        match self {
            &BoundingVolume::Box(min, max) => Aabb3::new(min, max),
            &BoundingVolume::Sphere(position, radius) => Aabb3::new(
                position.sub_element_wise(radius),
                position.add_element_wise(radius),
            ),
        }
    }
}
