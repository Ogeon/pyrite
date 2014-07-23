use cgmath::vector::{Vector, EuclideanVector, Vector2, Vector3};
use cgmath::point::Point;
use cgmath::transform::AffineMatrix3;
use cgmath::angle::{Angle, cos};
use cgmath::ray::{Ray, Ray3};

use tracer::{Camera, Area};

pub struct Perspective {
    transform: AffineMatrix3<f64>,
    image_size: Vector2<uint>,
    view_plane: f64
}

impl Perspective {
    pub fn new<A: Angle<f64>>(transform: AffineMatrix3<f64>, image_size: Vector2<uint>, fov: A) -> Perspective {
        let dist = cos(fov.div_s(2.0).to_rad());
        Perspective {
            transform: transform,
            image_size: image_size,
            view_plane: dist
        }
    }
}

impl Camera for Perspective {
    fn to_view_area(&self, area: &Area<uint>) -> Area<f64> {
        let float_image_size = Vector2::new(self.image_size.x as f64, self.image_size.y as f64);
        let float_coord = Vector2::new(area.from.x as f64, area.from.y as f64);
        let float_size = Vector2::new(area.size.x as f64, area.size.y as f64);

        let from = (float_coord.sub_v(&float_image_size.div_s(2.0))).div_s(float_image_size.comp_max() / 2.0);
        let size = float_size.div_s(float_image_size.comp_max() / 2.0);

        Area::new(from, size)
    }

    fn ray_towards(&self, target: &Vector2<f64>) -> Ray3<f64> {
        let mut direction = Vector3::new(target.x, target.y, self.view_plane);
        direction.normalize_self();
        Ray::new(Point::origin(), direction)
    }
}