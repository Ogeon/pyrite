use std::{error::Error, sync::Arc};

use rand::Rng;

use genmesh;
use obj;

use cgmath::{EuclideanSpace, InnerSpace, Matrix4, Point2, SquareMatrix, Vector3};
use collision::Ray3;

use crate::spatial::{bkd_tree, Dim3};

use crate::lamp::Lamp;
use crate::materials::Material;
use crate::{
    math::utils::ortho,
    project::{
        eval_context::{EvalContext, Evaluate, EvaluateOr},
        expressions::{Expression, Expressions},
        meshes::Meshes,
        program::ProgramCompiler,
        WorldObject,
    },
    shapes::{distance_estimators::QuatMul, BoundingVolume, Intersection, Shape, Triangle, Vertex},
    tracer::{LightProgram, ParametricValue},
};

pub struct World<'p> {
    pub sky: LightProgram<'p>,
    pub lights: Vec<Lamp<'p>>,
    pub objects: bkd_tree::BkdTree<Arc<Shape<'p>>>,
}

impl<'p> World<'p> {
    pub fn from_project(
        project: crate::project::World,
        eval_context: EvalContext,
        programs: ProgramCompiler<'p>,
        expressions: &Expressions,
        meshes: &Meshes,
    ) -> Result<Self, Box<dyn Error>> {
        let sky = programs.compile(&project.sky.unwrap_or(Expression::Number(0.0)), expressions)?;

        let mut objects: Vec<Arc<Shape>> = Vec::new();
        let mut lights = Vec::new();

        for (i, object) in project.objects.into_iter().enumerate() {
            match object {
                WorldObject::Sphere {
                    position,
                    radius,
                    material,
                } => {
                    let material =
                        Material::from_project(material, eval_context, programs, expressions)?;
                    let emissive = material.is_emissive();

                    let shape = Arc::new(Shape::Sphere {
                        position: position.evaluate(eval_context)?,
                        radius: radius.evaluate(eval_context)?,
                        material,
                    });

                    if emissive {
                        lights.push(Lamp::Shape(shape.clone()));
                    }
                    objects.push(shape);
                }
                WorldObject::Plane {
                    origin,
                    normal,
                    binormal,
                    texture_scale,
                    material,
                } => {
                    let normal = normal.evaluate(eval_context)?;
                    let tangent = ortho(normal).normalize();
                    let binormal = normal.cross(tangent).normalize();

                    let material =
                        Material::from_project(material, eval_context, programs, expressions)?;
                    let emissive = material.is_emissive();
                    let texture_scale: Option<_> = texture_scale.evaluate(eval_context)?;

                    let shape = Arc::new(Shape::Plane {
                        shape: collision::Plane::from_point_normal(
                            origin.evaluate(eval_context)?,
                            normal,
                        ),
                        tangent,
                        binormal,
                        texture_scale: texture_scale.unwrap_or(1.0),
                        material,
                    });

                    if emissive {
                        lights.push(Lamp::Shape(shape.clone()));
                    }
                    objects.push(shape);
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

                    let shape = Arc::new(Shape::RayMarched {
                        bounds,
                        estimator,
                        material,
                    });

                    if emissive {
                        lights.push(Lamp::Shape(shape.clone()));
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
                        println!("adding object '{}'", object.name);

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

        println!("the scene contains {} objects", objects.len());
        println!("building BKD-Tree... ");
        let tree = bkd_tree::BkdTree::new(objects, 10); //TODO: make arrity configurable
        println!("done building BKD-Tree");

        Ok(World {
            sky,
            lights,
            objects: tree,
        })
    }

    pub fn intersect(&self, ray: &Ray3<f32>) -> Option<(Intersection, &Material)> {
        self.objects.intersect(ray)
    }

    pub fn pick_lamp(&self, rng: &mut impl Rng) -> Option<(&Lamp, f32)> {
        self.lights
            .get(rng.gen_range(0, self.lights.len()))
            .map(|l| (l, 1.0 / self.lights.len() as f32))
    }
}

pub trait ObjectContainer {
    fn intersect(&self, ray: &Ray3<f32>) -> Option<(Intersection, &Material)>;
}

impl<'p> ObjectContainer for bkd_tree::BkdTree<Arc<Shape<'p>>> {
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
