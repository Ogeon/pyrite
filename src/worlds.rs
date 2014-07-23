use cgmath::ray::Ray3;
use cgmath::point::Point;
use cgmath::vector::EuclideanVector;

use tracer::World;

pub trait Scene {
    fn intersect(&self, ray: &Ray3<f64>) -> Option<(Ray3<f64>, f64)>;
}

pub trait WorldObject {
    fn intersect(&self, ray: &Ray3<f64>) -> Option<Ray3<f64>>;
}

impl<S: WorldObject> Scene for Vec<S> {
    fn intersect(&self, ray: &Ray3<f64>) -> Option<(Ray3<f64>, f64)> {
        self.iter().fold(None, |closest: Option<(Ray3<f64>, f64)>, object| {
            object.intersect(ray).map(|normal| {
                let new_dist = ray.origin.sub_p(&normal.origin).length2();
                match closest {
                    Some((closest_normal, closest_dist)) => {
                        if new_dist < closest_dist {
                            (normal, new_dist)
                        } else {
                            (closest_normal, closest_dist)
                        }
                    },
                    None => (normal, new_dist)
                }
            })
        })
    }
}

pub struct SimpleWorld<S> {
    scene: S
}

impl<S: Scene> SimpleWorld<S> {
    pub fn new(scene: S) -> SimpleWorld<S> {
        SimpleWorld {
            scene: scene
        }
    }
}

impl<S: Scene> World for SimpleWorld<S> {
    fn intersect(&self, ray: &Ray3<f64>) -> Option<(Ray3<f64>, f64)> {
        self.scene.intersect(ray)
    }
}