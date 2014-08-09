use std::collections::HashMap;

use cgmath::sphere;
use cgmath::vector::EuclideanVector;
use cgmath::point::{Point, Point3};
use cgmath::intersect::Intersect;
use cgmath::ray::{Ray, Ray3};

use tracer::Material;

use config;
use config::FromConfig;

pub enum Shape {
	Sphere { pub position: Point3<f64>, pub radius: f64, pub material: Box<Material + 'static + Send + Sync> }
}

impl Shape {
	pub fn intersect(&self, ray: &Ray3<f64>) -> Option<(Ray3<f64>, &Box<Material + Send + Sync>)> {
		match *self {
			Sphere {ref position, radius, ref material} => {
				let sphere = sphere::Sphere {
					radius: radius,
					center: position.clone()
				};
				(sphere, ray.clone())
					.intersection()
					.map(|intersection| Ray::new(intersection, intersection.sub_p(position).normalize()))
					.map(|normal| (normal, material))
			}
		}
	}
}



pub fn register_types(context: &mut config::ConfigContext) {
	context.insert_type("Shape", "Sphere", decode_sphere);
}

fn decode_sphere(context: &config::ConfigContext, items: HashMap<String, config::ConfigItem>) -> Result<Shape, String> {
    let mut items = items;

    let position = match items.pop_equiv(&"position") {
        Some(v) => try!(context.decode_structure_of_type("Vector", "3D", v), "position"),
        None => return Err(String::from_str("missing field 'position'"))
    };

    let radius = match items.pop_equiv(&"radius") {
        Some(v) => try!(FromConfig::from_config(v), "radius"),
        None => return Err(String::from_str("missing field 'radius'"))
    };

    let material = match items.pop_equiv(&"material") {
        Some(v) => try!(context.decode_structure_from_group("Material", v), "material"),
        None => return Err(String::from_str("missing field 'material'"))
    };

    Ok(Sphere {
    	position: Point::from_vec(&position),
    	radius: radius,
    	material: material
    })
}