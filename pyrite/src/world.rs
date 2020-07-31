use std::{error::Error, sync::Arc};

use rand::Rng;

use genmesh;
use obj;

use cgmath::{
    EuclideanSpace, InnerSpace, Matrix3, Matrix4, Point2, SquareMatrix, Vector2, Vector3,
};
use collision::Ray3;

use crate::{
    lamp::Lamp,
    materials::Material,
    math::DIST_EPSILON,
    program::ProgramCompiler,
    project::{
        eval_context::{EvalContext, Evaluate, EvaluateOr},
        expressions::{Expression, Expressions},
        meshes::Meshes,
        WorldObject,
    },
    shapes::{
        distance_estimators::QuatMul, BoundingVolume, Intersection, Normal, Plane, Shape, Triangle,
        Vertex,
    },
    spatial::bvh::Bvh,
    tracer::{LightProgram, ParametricValue},
};

pub(crate) struct World<'p> {
    pub sky: LightProgram<'p>,
    pub lights: Vec<Lamp<'p>>,
    pub planes: Vec<Plane<'p>>,
    pub finite_objects: Bvh<&'p Shape<'p>>,
}

impl<'p> World<'p> {
    pub fn from_project(
        project: crate::project::World,
        eval_context: EvalContext,
        programs: ProgramCompiler<'p>,
        expressions: &Expressions,
        meshes: &Meshes,
        allocator: &'p bumpalo::Bump,
    ) -> Result<Self, Box<dyn Error>> {
        let sky = programs.compile(&project.sky.unwrap_or(Expression::Number(0.0)), expressions)?;

        let mut objects: Vec<&Shape> = Vec::new();
        let mut planes = Vec::new();
        let mut lights = Vec::new();

        for (i, object) in project.objects.into_iter().enumerate() {
            match object {
                WorldObject::Sphere {
                    position,
                    radius,
                    texture_scale,
                    material,
                } => {
                    let material =
                        Material::from_project(material, eval_context, programs, expressions)?;
                    let emissive = material.is_emissive();
                    let texture_scale: Option<_> = texture_scale.evaluate(eval_context)?;

                    let shape = allocator.alloc(Shape::Sphere {
                        position: position.evaluate(eval_context)?,
                        radius: radius.evaluate(eval_context)?,
                        texture_scale: texture_scale.unwrap_or(Vector2::new(1.0, 1.0)),
                        material,
                    });

                    if emissive {
                        lights.push(Lamp::Shape(shape));
                    }
                    objects.push(shape);
                }
                WorldObject::Plane {
                    origin,
                    normal,
                    texture_scale,
                    material,
                } => {
                    let normal: Vector3<f32> = normal.evaluate(eval_context)?;
                    let normal = normal.normalize();
                    let (binormal, tangent) = crate::math::utils::basis(normal);

                    let material =
                        Material::from_project(material, eval_context, programs, expressions)?;
                    let emissive = material.is_emissive();
                    let texture_scale: Option<_> = texture_scale.evaluate(eval_context)?;

                    let shape = Plane {
                        shape: collision::Plane::from_point_normal(
                            origin.evaluate(eval_context)?,
                            normal,
                        ),
                        normal: Normal::new(
                            normal,
                            Matrix3::from_cols(binormal, tangent, normal).into(),
                        ),
                        texture_scale: texture_scale.unwrap_or(Vector2::new(1.0, 1.0)),
                        material,
                    };

                    if emissive {
                        println!(
                            "Warning: emissive planes may not always produce correct results."
                        );
                    }
                    planes.push(shape);
                }
                WorldObject::RayMarched {
                    shape,
                    bounds,
                    material,
                } => {
                    let material =
                        Material::from_project(material, eval_context, programs, expressions)?;
                    let emissive = material.is_emissive();

                    let bounds = match bounds {
                        crate::project::BoundingVolume::Box { min, max } => BoundingVolume::Box(
                            min.evaluate(eval_context)?,
                            max.evaluate(eval_context)?,
                        ),
                        crate::project::BoundingVolume::Sphere { position, radius } => {
                            BoundingVolume::Sphere(
                                position.evaluate(eval_context)?,
                                radius.evaluate(eval_context)?,
                            )
                        }
                    };

                    let estimator = match shape {
                        crate::project::Estimator::Mandelbulb {
                            iterations,
                            threshold,
                            power,
                            constant,
                        } => Box::new(crate::shapes::distance_estimators::Mandelbulb {
                            iterations: iterations.evaluate(eval_context)?,
                            threshold: threshold.evaluate(eval_context)?,
                            power: power.evaluate(eval_context)?,
                            constant: constant.map(|e| e.evaluate(eval_context)).transpose()?,
                        }) as Box<dyn ParametricValue<_, _>>,
                        crate::project::Estimator::QuaternionJulia {
                            iterations,
                            threshold,
                            constant,
                            slice_plane,
                            variant,
                        } => Box::new(crate::shapes::distance_estimators::QuaternionJulia {
                            iterations: iterations.evaluate(eval_context)?,
                            threshold: threshold.evaluate(eval_context)?,
                            constant: constant.evaluate(eval_context)?,
                            slice_plane: slice_plane.evaluate(eval_context)?,
                            ty: match &*variant.name {
                                "regular" => QuatMul::Regular,
                                "cubic" => QuatMul::Cubic,
                                "bicomplex" => QuatMul::Bicomplex,
                                name => {
                                    return Err(format!(
                                        "unexpected Julia fractal variant: {}",
                                        name
                                    )
                                    .into())
                                }
                            },
                        }) as Box<dyn ParametricValue<_, _>>,
                    };

                    let shape = allocator.alloc(Shape::RayMarched {
                        bounds,
                        estimator,
                        material,
                    });

                    if emissive {
                        println!("Warning: emissive, distance estimated shapes may not always produce correct results.");
                    }
                    objects.push(shape);
                }
                WorldObject::Mesh {
                    file,
                    mut materials,
                    scale,
                    transform,
                } => {
                    let transform =
                        transform.evaluate_or_else(eval_context, || Matrix4::identity())?;
                    let scale = scale.evaluate_or(eval_context, 1.0)?;
                    let obj = meshes.get(file);
                    for object in &obj.objects {
                        println!("Adding object '{}'.", object.name);

                        let (object_material, emissive) = match materials.remove(&object.name) {
                            Some(m) => {
                                let material =
                                    Material::from_project(m, eval_context, programs, expressions)?;
                                let emissive = material.is_emissive();
                                (Arc::new(material), emissive)
                            }
                            None => {
                                return Err(format!(
                                    "objects[{}]: missing material for '{}'",
                                    i, object.name
                                )
                                .into())
                            }
                        };

                        for group in &object.groups {
                            for shape in &group.polys {
                                match *shape {
                                    genmesh::Polygon::PolyTri(genmesh::Triangle { x, y, z }) => {
                                        let mut triangle =
                                            make_triangle(obj, x, y, z, object_material.clone());
                                        triangle.scale(scale);
                                        triangle.transform(transform);
                                        let triangle = allocator.alloc(triangle);
                                        if emissive {
                                            lights.push(Lamp::Shape(triangle));
                                        }

                                        objects.push(triangle);
                                    }
                                    _ => {}
                                }
                            }
                        }
                    }
                }
                WorldObject::DirectionalLight {
                    direction,
                    width,
                    color,
                } => lights.push(Lamp::Directional {
                    direction: direction.evaluate(eval_context)?,
                    width: width.evaluate(eval_context)?,
                    color: programs.compile(&color, expressions)?,
                }),
                WorldObject::PointLight { position, color } => lights.push(Lamp::Point(
                    position.evaluate(eval_context)?,
                    programs.compile(&color, expressions)?,
                )),
            }
        }

        println!(
            "The scene contains {} objects.",
            planes.len() + objects.len()
        );
        println!("Building BVH... ");
        let tree = Bvh::new(objects);
        println!("Done building BVH.");

        Ok(World {
            sky,
            lights,
            planes,
            finite_objects: tree,
        })
    }

    pub fn intersect(&self, ray: Ray3<f32>) -> Option<Intersection> {
        let mut result = None;
        let mut closest_distance = f32::INFINITY;

        for plane in &self.planes {
            if let Some(intersection) = plane.ray_intersect(&ray) {
                if intersection.distance > DIST_EPSILON && intersection.distance < closest_distance
                {
                    closest_distance = intersection.distance;
                    result = Some(intersection);
                }
            }
        }

        let mut intersections = self.finite_objects.ray_intersect(ray);
        while let Some(&object) = intersections.next(closest_distance) {
            if let Some(intersection) = object.ray_intersect(&ray) {
                if intersection.distance > DIST_EPSILON && intersection.distance < closest_distance
                {
                    closest_distance = intersection.distance;
                    result = Some(intersection);
                }
            }
        }

        result
    }

    pub fn pick_lamp(&self, rng: &mut impl Rng) -> Option<(&Lamp, f32)> {
        self.lights
            .get(rng.gen_range(0, self.lights.len()))
            .map(|l| (l, 1.0 / self.lights.len() as f32))
    }
}

fn make_triangle<'p, M: obj::GenPolygon>(
    obj: &obj::Obj<'_, M>,
    obj::IndexTuple(v1, t1, n1): obj::IndexTuple,
    obj::IndexTuple(v2, t2, n2): obj::IndexTuple,
    obj::IndexTuple(v3, t3, n3): obj::IndexTuple,
    material: Arc<Material<'p>>,
) -> Shape<'p> {
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

    let delta_position1 = v2 - v1;
    let delta_position2 = v3 - v1;

    let delta_texture1 = t2 - t1;
    let delta_texture2 = t3 - t1;

    let r = 1.0 / (delta_texture1.x * delta_texture2.y - delta_texture1.y * delta_texture2.x);
    let tangent = (delta_position1 * delta_texture2.y - delta_position2 * delta_texture1.y) * r;
    let bitangent = (delta_position2 * delta_texture1.x - delta_position1 * delta_texture2.x) * r;

    Triangle {
        v1: Vertex {
            position: v1,
            normal: Normal::new(n1, Matrix3::from_cols(tangent, bitangent, n1).into()),
            texture: t1,
        },
        v2: Vertex {
            position: v2,
            normal: Normal::new(n2, Matrix3::from_cols(tangent, bitangent, n2).into()),
            texture: t2,
        },
        v3: Vertex {
            position: v3,
            normal: Normal::new(n3, Matrix3::from_cols(tangent, bitangent, n3).into()),
            texture: t3,
        },
        edge1: delta_position1,
        edge2: delta_position2,
        material,
    }
}
