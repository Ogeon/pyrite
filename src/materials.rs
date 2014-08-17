use std;
use std::collections::HashMap;

use cgmath::vector::{EuclideanVector, Vector, Vector3};
use cgmath::ray::{Ray, Ray3};

use tracer;
use tracer::{Material, FloatRng, Reflection, ParametricValue, Emit, Reflect};

use config;
use config::FromConfig;

use math;

pub struct Diffuse {
    pub color: Box<ParametricValue<tracer::RenderContext, f64> + 'static + Send + Sync>
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
            &if n.x.abs() < n.y.abs() && n.x.abs() < n.z.abs() {
                Vector3::new(n.x.signum(), 0.0, 0.0)
            } else if n.y.abs() < n.z.abs() {
                Vector3::new(0.0, n.y.signum(), 0.0)
            } else {
                Vector3::new(0.0, 0.0, n.z.signum())
            }
        );

        reflected.normalize_self_to(sphere_point.x);

        let mut y = n.cross(&reflected);
        y.normalize_self_to(sphere_point.y);

        reflected.add_self_v(&y);

        n.normalize_self_to(sphere_point.z);
        reflected.add_self_v(&n);

        Reflect(Ray::new(normal.origin, reflected), &self.color as &ParametricValue<tracer::RenderContext, f64>, 1.0)
    }
}

pub struct Emission {
    pub color: Box<ParametricValue<tracer::RenderContext, f64> + 'static + Send + Sync>
}

impl Material for Emission {
    fn reflect(&self, _ray_in: &Ray3<f64>, _normal: &Ray3<f64>, _rng: &mut FloatRng) -> Reflection {
        Emit(&self.color as &ParametricValue<tracer::RenderContext, f64>)
    }
}

pub struct Mirror {
    pub color: Box<ParametricValue<tracer::RenderContext, f64> + 'static + Send + Sync>
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
        Reflect(Ray::new(normal.origin, ray_in.direction.sub_v(&n)), &self.color as &ParametricValue<tracer::RenderContext, f64>, 1.0)
    }
}

pub struct Mix {
    factor: f64,
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

struct FresnelMix {
    ior: f64,
    env_ior: f64,
    pub reflect: Box<Material + 'static + Send + Sync>,
    pub refract: Box<Material + 'static + Send + Sync>
}

impl Material for FresnelMix {
    fn reflect(&self, ray_in: &Ray3<f64>, normal: &Ray3<f64>, rng: &mut FloatRng) -> Reflection {
        let factor = if ray_in.direction.dot(&normal.direction) < 0.0 {
            math::utils::schlick(self.env_ior, self.ior, &normal.direction, &ray_in.direction)
        } else {
            math::utils::schlick(self.ior, self.env_ior, &-normal.direction, &ray_in.direction)
        };

        if factor > rng.next_float() {
            self.reflect.reflect(ray_in, normal, rng)
        } else {
            self.refract.reflect(ray_in, normal, rng)
        }
    }
}

struct Refractive {
    color: Box<ParametricValue<tracer::RenderContext, f64> + 'static + Send + Sync>,
    ior: f64,
    env_ior: f64
}

impl Material for Refractive {
    fn reflect(&self, ray_in: &Ray3<f64>, normal: &Ray3<f64>, rng: &mut FloatRng) -> Reflection {
        let nl = if normal.direction.dot(&ray_in.direction) < 0.0 {
            normal.direction
        } else {
            -normal.direction
        };

        let reflected = ray_in.direction.sub_v(&normal.direction.mul_s(2.0 * normal.direction.dot(&ray_in.direction)));

        let into = normal.direction.dot(&nl) > 0.0;

        let nc = self.env_ior;
        let nt = self.ior;

        let nnt = if into { nc/nt } else { nt/nc };
        let ddn = ray_in.direction.dot(&nl);
        
        let cos2t = 1.0 - nnt * nnt * (1.0 - ddn * ddn);
        if cos2t < 0.0 { // Total internal reflection
            return Reflect(Ray::new(normal.origin, reflected), self.color, 1.0);
        }

        let s = if into { 1.0 } else { -1.0 }*(ddn * nnt + cos2t.sqrt());
        let tdir = ray_in.direction.mul_s(nnt).sub_v(&normal.direction.mul_s(s)).normalize();

        let a = nt - nc;
        let b = nt + nc;
        let r0 = a * a / (b * b);
        let c = 1.0 - if into { -ddn } else { tdir.dot(&normal.direction) };

        let re = r0 + (1.0 - r0) * c*c*c*c*c;
        let tr = 1.0 - re;
        let p = 0.25 + 0.5 * re;
        let rp = re / p;
        let tp = tr / (1.0 - p);

        if rng.next_float() < p {
            return Reflect(Ray::new(normal.origin, reflected), self.color, rp);
        } else {
            return Reflect(Ray::new(normal.origin, tdir), self.color, tp);
        }
    }
}





pub fn register_types(context: &mut config::ConfigContext) {
    context.insert_grouped_type("Material", "Diffuse", decode_diffuse);
    context.insert_grouped_type("Material", "Emission", decode_emission);
    context.insert_grouped_type("Material", "Refractive", decode_refractive);
    context.insert_grouped_type("Material", "Mirror", decode_mirror);
    context.insert_grouped_type("Material", "Mix", decode_mix);
    context.insert_grouped_type("Material", "FresnelMix", decode_fresnel_mix);
}

pub fn decode_diffuse(context: &config::ConfigContext, fields: HashMap<String, config::ConfigItem>) -> Result<Box<Material + 'static + Send + Sync>, String> {
    let mut fields = fields;

    let color = match fields.pop_equiv(&"color") {
        Some(v) => try!(tracer::decode_parametric_number(context, v), "color"),
        None => return Err(String::from_str("missing field 'color'"))
    };

    Ok(box Diffuse { color: color } as Box<Material + 'static + Send + Sync>)
}

pub fn decode_emission(context: &config::ConfigContext, fields: HashMap<String, config::ConfigItem>) -> Result<Box<Material + 'static + Send + Sync>, String> {
    let mut fields = fields;

    let color = match fields.pop_equiv(&"color") {
        Some(v) => try!(tracer::decode_parametric_number(context, v), "color"),
        None => return Err(String::from_str("missing field 'color'"))
    };

    Ok(box Emission { color: color } as Box<Material + 'static + Send + Sync>)
}

pub fn decode_mirror(context: &config::ConfigContext, fields: HashMap<String, config::ConfigItem>) -> Result<Box<Material + 'static + Send + Sync>, String> {
    let mut fields = fields;

    let color = match fields.pop_equiv(&"color") {
        Some(v) => try!(tracer::decode_parametric_number(context, v), "color"),
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

pub fn decode_fresnel_mix(context: &config::ConfigContext, fields: HashMap<String, config::ConfigItem>) -> Result<Box<Material + 'static + Send + Sync>, String> {
    let mut fields = fields;

    let ior = match fields.pop_equiv(&"ior") {
        Some(v) => try!(FromConfig::from_config(v), "ior"),
        None => return Err(String::from_str("missing field 'ior'"))
    };

    let env_ior = match fields.pop_equiv(&"env_ior") {
        Some(v) => try!(FromConfig::from_config(v), "env_ior"),
        None => 1.0
    };

    let reflect = match fields.pop_equiv(&"reflect") {
        Some(v) => try!(context.decode_structure_from_group("Material", v), "reflect"),
        None => return Err(String::from_str("missing field 'reflect'"))
    };

    let refract = match fields.pop_equiv(&"refract") {
        Some(v) => try!(context.decode_structure_from_group("Material", v), "refract"),
        None => return Err(String::from_str("missing field 'refract'"))
    };

    Ok(box FresnelMix {
        ior: ior,
        env_ior: env_ior,
        reflect: reflect,
        refract: refract
    } as Box<Material + 'static + Send + Sync>)
}

pub fn decode_refractive(context: &config::ConfigContext, fields: HashMap<String, config::ConfigItem>) -> Result<Box<Material + 'static + Send + Sync>, String> {
    let mut fields = fields;

    let ior = match fields.pop_equiv(&"ior") {
        Some(v) => try!(FromConfig::from_config(v), "ior"),
        None => return Err(String::from_str("missing field 'ior'"))
    };

    let env_ior = match fields.pop_equiv(&"env_ior") {
        Some(v) => try!(FromConfig::from_config(v), "env_ior"),
        None => 1.0
    };

    let color = match fields.pop_equiv(&"color") {
        Some(v) => try!(tracer::decode_parametric_number(context, v), "color"),
        None => return Err(String::from_str("missing field 'color'"))
    };

    Ok(box Refractive {
        ior: ior,
        env_ior: env_ior,
        color: color
    } as Box<Material + 'static + Send + Sync>)
}