use cgmath::vector::{Vector, Vector2, Vector3};
use cgmath::transform::AffineMatrix3;
use cgmath::angle::{Angle, cos};

use tracer::{Camera, Area};

pub struct Perspective {
    transform: AffineMatrix3<f64>,
    image_size: Vector2<uint>,
    view_plane: Vector3<f64>
}

impl Perspective {
    pub fn new<A: Angle<f64>>(transform: AffineMatrix3<f64>, image_size: Vector2<uint>, fov: A) -> Perspective {
        let float_size = Vector2::new(image_size.x as f64, image_size.y as f64);
        let dist = cos(fov.div_s(2.0).to_rad());
        let size = (float_size.sub_v(&float_size.div_s(2.0))).div_s(float_size.comp_max() / 2.0);
        Perspective {
            transform: transform,
            image_size: image_size,
            view_plane: Vector3::new(size.x, size.y, dist)
        }
    }
}

impl Camera for Perspective {
    fn to_view_area(&self, area: Area<uint>) -> Area<f64> {
        let float_image_size = Vector2::new(self.image_size.x as f64, self.image_size.y as f64);
        let float_coord = Vector2::new(area.from.x as f64, area.from.y as f64);
        let float_size = Vector2::new(area.size.x as f64, area.size.y as f64);

        let from = (float_coord.sub_v(&float_image_size.div_s(2.0))).div_s(float_image_size.comp_max() / 2.0);
        let size = float_size.div_s(float_image_size.comp_max() / 2.0);

        Area::new(from, size)
    }
}