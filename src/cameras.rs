use std::f64::consts;

use rand::Rng;

use cgmath::{Vector, EuclideanVector, Vector2};
use cgmath::{Point, Point3};
use cgmath::{AffineMatrix3, Transform};
use cgmath::{Angle, Rad, cos, sin, deg};
use cgmath::{Ray, Ray3};

use renderer::Area;

use config::Prelude;
use config::entry::Entry;

pub fn register_types(context: &mut Prelude) {
    context.object("Camera".into()).object("Perspective".into()).add_decoder(decode_perspective);
}

pub enum Camera {
    Perspective {
        transform: AffineMatrix3<f64>,
        view_plane: f64,
        focus_distance: f64,
        aperture: f64
    }
}

impl Camera {
    pub fn to_view_area(&self, area: &Area<u32>, image_size: &Vector2<u32>) -> Area<f64> {
        let float_image_size = Vector2::new(image_size.x as f64, image_size.y as f64);
        let float_coord = Vector2::new(area.from.x as f64, area.from.y as f64);
        let float_size = Vector2::new(area.size.x as f64, area.size.y as f64);

        let from = (float_coord.sub_v(&float_image_size.div_s(2.0))).div_s(float_image_size.comp_max() / 2.0);
        let size = float_size.div_s(float_image_size.comp_max() / 2.0);

        Area::new(from, size)
    }

    pub fn ray_towards<R: Rng>(&self, target: &Vector2<f64>, rng: &mut R) -> Ray3<f64> {
        match *self {
            Camera::Perspective { ref transform, view_plane, focus_distance, aperture } => {
                let focus_x = target.x / view_plane * focus_distance;
                let focus_y = target.y / view_plane * focus_distance;

                let target = Point3::new(focus_x, -focus_y, -focus_distance);

                let (origin, mut direction) = if aperture > 0.0 {
                    let sqrt_r = (aperture * rng.gen::<f64>()).sqrt();
                    let psi = consts::PI * 2.0 * rng.gen::<f64>();
                    let lens_x = sqrt_r * psi.cos();
                    let lens_y = sqrt_r * psi.sin();
                    let origin = Point3::new(lens_x, lens_y, 0.0);
                    (origin, target.sub_p(&origin))
                } else {
                    (Point::origin(), target.to_vec())
                };

                direction.normalize_self();
                transform.transform_ray(&Ray::new(origin, direction))
            }
        }
    }
}

fn decode_perspective(entry: Entry) -> Result<Camera, String> {
    let items = try!(entry.as_object().ok_or("not an object".into()));

    let transform = match items.get("transform") {
        Some(v) => AffineMatrix3 {
            mat: try!(v.dynamic_decode(), "transform")
        },
        None => Transform::identity()
    };

    let fov: f64 = match items.get("fov") {
        Some(v) => try!(v.decode(), "fov"),
        None => return Err("missing field of view ('fov')".into())
    };

    let focus_distance: f64 = match items.get("focus_distance") {
        Some(v) => try!(v.decode(), "focus_distance"),
        None => 1.0
    };

    let aperture: f64 = match items.get("aperture") {
        Some(v) => try!(v.decode(), "aperture"),
        None => 0.0
    };

    let a: Rad<_> = deg(fov / 2.0).into();
    let dist = cos(a) / sin(a);

    Ok(Camera::Perspective {
        transform: transform,
        view_plane: dist,
        focus_distance: focus_distance,
        aperture: aperture
    })
}