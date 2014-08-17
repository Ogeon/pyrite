use std::collections::HashMap;

use cgmath::{Vector, EuclideanVector, Vector2, Vector3};
use cgmath::Point;
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
        view_plane: f64
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

    pub fn ray_towards(&self, target: &Vector2<f64>) -> Ray3<f64> {
        match *self {
            Perspective { transform: ref transform, view_plane: view_plane} => {
                let mut direction = Vector3::new(target.x, -target.y, -view_plane);
                direction.normalize_self();
                transform.transform_ray(&Ray::new(Point::origin(), direction))
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

    let a = deg(fov / 2.0).to_rad();
    let dist = cos(a) / sin(a);

    Ok(Perspective {
        transform: transform,
        view_plane: dist
    })
}