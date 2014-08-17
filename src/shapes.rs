use std::collections::HashMap;
use std::sync::Arc;

use cgmath;
use cgmath::{Vector, EuclideanVector};
use cgmath::{Point, Point3};
use cgmath::Intersect;
use cgmath::{Ray, Ray3};

use tracer;
use tracer::Material;

use config;
use config::{FromConfig, Type};

use bkdtree;

pub enum ProxyShape {
	DecodedShape { pub shape: Shape },
	Mesh { pub file: String, pub materials: HashMap<String, config::ConfigItem> }
}

pub enum Shape {
	Sphere { position: Point3<f64>, radius: f64, material: Box<Material + 'static + Send + Sync> },
	Triangle { pub v1: Point3<f64>, pub v2: Point3<f64>, pub v3: Point3<f64>, pub material: Arc<Box<Material + 'static + Send + Sync>> }
}

impl Shape {
	pub fn intersect(&self, ray: &Ray3<f64>) -> Option<(f64, Ray3<f64>)> {
		match *self {
			Sphere {ref position, radius, ..} => {
				let sphere = cgmath::Sphere {
					radius: radius,
					center: position.clone()
				};
				(sphere, ray.clone())
					.intersection()
					.map(|intersection| (intersection.sub_p(&ray.origin).length(), Ray::new(intersection, intersection.sub_p(position).normalize())) )
			},
			Triangle {ref v1, ref v2, ref v3, ..} => {
				//Möller–Trumbore intersection algorithm
				let epsilon = 0.000001f64;
				let e1 = v2.sub_p(v1);
				let e2 = v3.sub_p(v1);

				let p = ray.direction.cross(&e2);
				let det = e1.dot(&p);

				if det > -epsilon && det < epsilon {
					return None;
				}

				let inv_det = 1.0 / det;
				let t = ray.origin.sub_p(v1);
				let u = t.dot(&p) * inv_det;

				//Outside triangle
				if u < 0.0 || u > 1.0 {
					return None;
				}

				let q = t.cross(&e1);
				let v = ray.direction.dot(&q) * inv_det;

				//Outside triangle
				if v < 0.0 || u + v > 1.0 {
					return None;
				}

				let dist = e2.dot(&q) * inv_det;
				if dist > epsilon {
					let hit_position = ray.origin.add_v(&ray.direction.mul_s(dist));
					Some(( dist, Ray::new(hit_position, e1.cross(&e2).normalize()) ))
				} else {
					None
				}
			}
		}
	}

	pub fn get_material(&self) -> &Material {
		match *self {
    		Sphere { ref material, .. } => material as &Material,
    		Triangle { ref material, .. } => material.deref() as &Material
    	}
	}
}

impl<'a> bkdtree::Element<tracer::BkdRay<'a>, Ray3<f64>> for Shape {
    fn get_bounds_interval(&self, axis: uint) -> (f64, f64) {
    	match *self {
    		Sphere { ref position, radius, .. } => match axis {
    			0 => (position.x - radius, position.x + radius),
    			1 => (position.y - radius, position.y + radius),
    			_ => (position.z - radius, position.z + radius)
    		},
    		Triangle { ref v1, ref v2, ref v3, .. } => {
    			let min = v1.min(v2).min(v3);
    			let max = v1.max(v2).max(v3);

    			match axis {
	    			0 => (min.x, max.x),
	    			1 => (min.y, max.y),
	    			_ => (min.z, max.z)
	    		}
    		}
    	}
    }

    fn intersect(&self, ray: &tracer::BkdRay) -> Option<(f64, Ray3<f64>)> {
    	let &tracer::BkdRay(ray) = ray;
    	self.intersect(ray)
    }
}



pub fn register_types(context: &mut config::ConfigContext) {
	context.insert_grouped_type("Shape", "Sphere", decode_sphere);
	context.insert_grouped_type("Shape", "Mesh", decode_mesh);
}

fn decode_sphere(context: &config::ConfigContext, items: HashMap<String, config::ConfigItem>) -> Result<ProxyShape, String> {
    let mut items = items;

    let position = match items.pop_equiv(&"position") {
        Some(v) => try!(context.decode_structure_of_type(&Type::single("Vector"), v), "position"),
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

    Ok(DecodedShape {
    	shape: Sphere {
	    	position: Point::from_vec(&position),
	    	radius: radius,
	    	material: material
	    }
	})
}

fn decode_mesh(_context: &config::ConfigContext, items: HashMap<String, config::ConfigItem>) -> Result<ProxyShape, String> {
	let mut items = items;

    let file_name: String = match items.pop_equiv(&"file") {
        Some(v) => try!(FromConfig::from_config(v), "file"),
        None => return Err(String::from_str("missing field 'file'"))
    };

    let materials = match items.pop_equiv(&"materials") {
        Some(config::Structure(_, fields)) => fields,
        Some(v) => return Err(format!("materials: expected a structure, but found '{}'", v)),
        None => return Err(String::from_str("missing field 'materials'"))
    };

    Ok(Mesh{
    	file: file_name,
    	materials: materials
    })
}