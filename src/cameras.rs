use std::collections::HashMap;
use std::simd;
use std::f64::consts;
use std::rand::Rng;

use cgmath::{Vector, EuclideanVector, Vector2};
use cgmath::{Point, Point3};
use cgmath::{AffineMatrix3, Transform};
use cgmath::{Angle, ToRad, cos, sin, deg};
use cgmath::{Ray, Ray3};

use renderer::Area;

use config;
use config::FromConfig;

pub fn register_types(context: &mut config::ConfigContext) {
    context.insert_grouped_type("Camera", "Perspective", decode_perspective);
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
    pub fn to_view_area(&self, area: &Area<uint>, image_size: &Vector2<uint>) -> Area<f64> {
        let float_image_size = Vector2::new(image_size.x as f64, image_size.y as f64);
        let float_coord = Vector2::new(area.from.x as f64, area.from.y as f64);
        let float_size = Vector2::new(area.size.x as f64, area.size.y as f64);

        let from = (float_coord.sub_v(&float_image_size.div_s(2.0))).div_s(float_image_size.comp_max() / 2.0);
        let size = float_size.div_s(float_image_size.comp_max() / 2.0);

        Area::new(from, size)
    }

    pub fn ray_towards<R: Rng>(&self, target: &Vector2<f64>, rng: &mut R) -> Ray3<f64> {
        match *self {
            Perspective { ref transform, view_plane, focus_distance, aperture } => {
                let v_plane = simd::f64x2(view_plane, view_plane);
                let f_distance = simd::f64x2(focus_distance, focus_distance);
                let target = simd::f64x2(target.x, target.y);
                let simd::f64x2(focus_x, focus_y) = target / v_plane * f_distance;

                let target = Point3::new(focus_x, -focus_y, -focus_distance);

                let (origin, mut direction) = if aperture > 0.0 {
                    let sqrt_r = (aperture * rng.gen()).sqrt();
                    let psi = consts::PI * 2.0 * rng.gen();
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

fn decode_perspective(context: &config::ConfigContext, items: HashMap<String, config::ConfigItem>) -> Result<Camera, String> {
    let mut items = items;

    let transform = match items.pop_equiv(&"transform") {
        Some(v) => AffineMatrix3 {
            mat: try!(context.decode_structure_from_group("Transform", v), "transform")
        },
        None => Transform::identity()
    };

    let fov: f64 = match items.pop_equiv(&"fov") {
        Some(v) => try!(FromConfig::from_config(v), "fov"),
        None => return Err(String::from_str("missing field of view ('fov')"))
    };

    let focus_distance: f64 = match items.pop_equiv(&"focus_distance") {
        Some(v) => try!(FromConfig::from_config(v), "focus_distance"),
        None => 1.0
    };

    let aperture: f64 = match items.pop_equiv(&"aperture") {
        Some(v) => try!(FromConfig::from_config(v), "aperture"),
        None => 0.0
    };

    let a = deg(fov / 2.0).to_rad();
    let dist = cos(a) / sin(a);

    Ok(Perspective {
        transform: transform,
        view_plane: dist,
        focus_distance: focus_distance,
        aperture: aperture
    })
}