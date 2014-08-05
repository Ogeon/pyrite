use cgmath::ray::Ray3;
use cgmath::point::Point;
use cgmath::vector::{EuclideanVector, Vector3};

use tracer::{World, Material, ParametricValue};

pub trait Scene {
    fn intersect(&self, ray: &Ray3<f64>) -> Option<(Ray3<f64>, f64, &Material)>;
}

pub trait WorldObject {
    fn intersect(&self, ray: &Ray3<f64>) -> Option<(Ray3<f64>, &Material)>;
}

impl<S: WorldObject> Scene for Vec<S> {
    fn intersect(&self, ray: &Ray3<f64>) -> Option<(Ray3<f64>, f64, &Material)> {
        let mut closest: Option<(Ray3<f64>, f64, &Material)> = None;

        for object in self.iter() {
            closest = object.intersect(ray).map(|(normal, material)| {

                let new_dist = ray.origin.sub_p(&normal.origin).length2();

                match closest {
                    Some((closest_normal, closest_dist, closest_material)) => {
                        if new_dist < closest_dist {
                            (normal, new_dist, material)
                        } else {
                            (closest_normal, closest_dist, closest_material)
                        }
                    },
                    None => (normal, new_dist, material)
                }

            }).or(closest);
        }

        closest
    }
}

pub struct SimpleWorld<S, C> {
    scene: S,
    sky_color: C
}

impl<S: Scene, C: ParametricValue<f64, f64> + Send + Share> SimpleWorld<S, C> {
    pub fn new(scene: S, sky_color: C) -> SimpleWorld<S, C> {
        SimpleWorld {
            scene: scene,
            sky_color: sky_color
        }
    }
}

impl<S: Scene, C: ParametricValue<f64, f64> + 'static> World for SimpleWorld<S, C> {
    fn intersect(&self, ray: &Ray3<f64>) -> Option<(Ray3<f64>, f64, &Material)> {
        self.scene.intersect(ray)
    }

    fn sky_color(&self, _direction: &Vector3<f64>) -> &ParametricValue<f64, f64> {
        &self.sky_color as &ParametricValue<f64, f64>
    }
}