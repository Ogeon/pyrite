use std;
use std::collections::HashMap;

use cgmath::vector::{EuclideanVector, Vector, Vector3};
use cgmath::ray::{Ray, Ray3};

use tracer::{Material, FloatRng, Reflection, ParametricValue, Emit, Reflect};

use config;
use config::FromConfig;

pub struct Diffuse {
    pub color: Box<ParametricValue<f64, f64> + 'static + Send + Sync>
}

impl Material for Diffuse {
    fn reflect(&self, ray_in: &Ray3<f64>, normal: &Ray3<f64>, rng: &mut FloatRng) -> Reflection {
        let u = rng.next_float();
        let v = rng.next_float();
        let theta = 2.0f64 * std::f64::consts::PI * u;
        let phi = (2.0 * v - 1.0).acos();
        let sphere_point = Vector3::new(
            phi.sin() * theta.cos(),
            phi.sin() * theta.sin(),
            phi.cos().abs()
            );

        let mut n = if ray_in.direction.dot(&normal.direction) < 0.0 {
            normal.direction
        } else {
            -normal.direction
        };

        let mut reflected = n.cross(
            &if n.x >= n.y && n.x >= n.z {
                Vector3::new(1.0, 0.0, 0.0)
            } else if n.y >= n.z {
                Vector3::new(0.0, 1.0, 0.0)
            } else {
                Vector3::new(0.0, 0.0, 1.0)
            }
        );

        reflected.normalize_self_to(sphere_point.x);

        let mut y = n.cross(&reflected);
        y.normalize_self_to(sphere_point.y);

        reflected.add_self_v(&y);

        n.normalize_self_to(sphere_point.z);
        reflected.add_self_v(&n);

        Reflect(Ray::new(normal.origin, reflected), &self.color as &ParametricValue<f64, f64>)
    }
}

pub struct Emission {
    pub color: Box<ParametricValue<f64, f64> + 'static + Send + Sync>
}

impl Material for Emission {
    fn reflect(&self, _ray_in: &Ray3<f64>, _normal: &Ray3<f64>, _rng: &mut FloatRng) -> Reflection {
        Emit(&self.color as &ParametricValue<f64, f64>)
    }
}

pub struct Mirror {
    pub color: Box<ParametricValue<f64, f64> + 'static + Send + Sync>
}

impl Material for Mirror {
    fn reflect(&self, ray_in: &Ray3<f64>, normal: &Ray3<f64>, _rng: &mut FloatRng) -> Reflection {

        let mut n = if ray_in.direction.dot(&normal.direction) < 0.0 {
            normal.direction
        } else {
            -normal.direction
        };

        let perp = ray_in.direction.dot(&n) * 2.0;
        n.mul_self_s(perp);
        Reflect(Ray::new(normal.origin, ray_in.direction.sub_v(&n)), &self.color as &ParametricValue<f64, f64>)
    }
}

pub struct Mix {
    pub factor: f64,
    pub a: Box<Material + 'static + Send + Sync>,
    pub b: Box<Material + 'static + Send + Sync>
}

impl Material for Mix {
    fn reflect(&self, ray_in: &Ray3<f64>, normal: &Ray3<f64>, rng: &mut FloatRng) -> Reflection {
        if self.factor < rng.next_float() {
            self.a.reflect(ray_in, normal, rng)
        } else {
            self.b.reflect(ray_in, normal, rng)
        }
    }
}



pub fn register_types(context: &mut config::ConfigContext) {
    context.insert_type("Material", "Diffuse", decode_diffuse);
    context.insert_type("Material", "Emission", decode_emission);
    context.insert_type("Material", "Mirror", decode_mirror);
    context.insert_type("Material", "Mix", decode_mix);
}

pub fn decode_diffuse(context: &config::ConfigContext, fields: HashMap<String, config::ConfigItem>) -> Result<Box<Material + 'static + Send + Sync>, String> {
    let mut fields = fields;

    let color = match fields.pop_equiv(&"color") {
        Some(config::Primitive(config::Number(n))) => box n as Box<ParametricValue<f64, f64> + 'static + Send + Sync>,
        Some(v) => return Err(String::from_str("only numbers are accepted for field 'color'")),//Todo: try!(FromConfig::from_config(v), "color"),
        None => return Err(String::from_str("missing field 'color'"))
    };

    Ok(box Diffuse { color: color } as Box<Material + 'static + Send + Sync>)
}

pub fn decode_emission(context: &config::ConfigContext, fields: HashMap<String, config::ConfigItem>) -> Result<Box<Material + 'static + Send + Sync>, String> {
    let mut fields = fields;

    let color = match fields.pop_equiv(&"color") {
        Some(config::Primitive(config::Number(n))) => box n as Box<ParametricValue<f64, f64> + 'static + Send + Sync>,
        Some(v) => return Err(String::from_str("only numbers are accepted for field 'color'")),//Todo: try!(FromConfig::from_config(v), "color"),
        None => return Err(String::from_str("missing field 'color'"))
    };

    Ok(box Emission { color: color } as Box<Material + 'static + Send + Sync>)
}

pub fn decode_mirror(context: &config::ConfigContext, fields: HashMap<String, config::ConfigItem>) -> Result<Box<Material + 'static + Send + Sync>, String> {
    let mut fields = fields;

    let color = match fields.pop_equiv(&"color") {
        Some(config::Primitive(config::Number(n))) => box n as Box<ParametricValue<f64, f64> + 'static + Send + Sync>,
        Some(v) => return Err(String::from_str("only numbers are accepted for field 'color'")),//Todo: try!(FromConfig::from_config(v), "color"),
        None => return Err(String::from_str("missing field 'color'"))
    };

    Ok(box Mirror { color: color } as Box<Material + 'static + Send + Sync>)
}

pub fn decode_mix(context: &config::ConfigContext, fields: HashMap<String, config::ConfigItem>) -> Result<Box<Material + 'static + Send + Sync>, String> {
    let mut fields = fields;

    let factor = match fields.pop_equiv(&"factor") {
        Some(v) => try!(FromConfig::from_config(v), "factor"),
        None => return Err(String::from_str("missing field 'factor'"))
    };

    let a = match fields.pop_equiv(&"a") {
        Some(v) => try!(context.decode_structure_from_group("Material", v), "a"),
        None => return Err(String::from_str("missing field 'a'"))
    };

    let b = match fields.pop_equiv(&"b") {
        Some(v) => try!(context.decode_structure_from_group("Material", v), "b"),
        None => return Err(String::from_str("missing field 'b'"))
    };

    Ok(box Mix {
        factor: factor,
        a: a,
        b: b
    } as Box<Material + 'static + Send + Sync>)
}