use rand::Rng;

use cgmath::{EuclideanVector, Vector, Vector3};
use cgmath::{Ray, Ray3};

use tracer;
use tracer::{Material, Reflection, ParametricValue, Emit, Reflect, Light, Color};

use config::Prelude;
use config::entry::Entry;

use math;

pub type MaterialBox = Box<Material + 'static + Send + Sync>;

pub struct Diffuse {
    pub color: Box<Color>
}

impl Material for Diffuse {
    fn reflect(&self, _light: &mut Light, ray_in: &Ray3<f64>, normal: &Ray3<f64>, rng: &mut Rng) -> Reflection {
        let n = if ray_in.direction.dot(&normal.direction) < 0.0 {
            normal.direction
        } else {
            -normal.direction
        };

        let reflected = math::utils::sample_hemisphere(rng, &n);
        Reflect(Ray::new(normal.origin, reflected), & *self.color, 1.0, Some(lambertian))
    }

    fn get_emission(&self, _light: &mut Light, _ray_in: &Vector3<f64>, _normal: &Ray3<f64>, _rng: &mut Rng) -> Option<&Color> {
        None
    }
}

fn lambertian(_ray_in: &Vector3<f64>, ray_out: &Vector3<f64>, normal: &Vector3<f64>) -> f64 {
    2.0 * normal.dot(ray_out).abs()
}

pub struct Emission {
    pub color: Box<Color>
}

impl Material for Emission {
    fn reflect(&self, _light: &mut Light, _ray_in: &Ray3<f64>, _normal: &Ray3<f64>, _rng: &mut Rng) -> Reflection {
        Emit(& *self.color)
    }

    fn get_emission(&self, _light: &mut Light, _ray_in: &Vector3<f64>, _normal: &Ray3<f64>, _rng: &mut Rng) -> Option<&Color> {
        Some(& *self.color)
    }
}

pub struct Mirror {
    pub color: Box<Color>
}

impl Material for Mirror {
    fn reflect(&self, _light: &mut Light, ray_in: &Ray3<f64>, normal: &Ray3<f64>, _rng: &mut Rng) -> Reflection {

        let mut n = if ray_in.direction.dot(&normal.direction) < 0.0 {
            normal.direction
        } else {
            -normal.direction
        };

        let perp = ray_in.direction.dot(&n) * 2.0;
        n.mul_self_s(perp);
        Reflect(Ray::new(normal.origin, ray_in.direction.sub_v(&n)), & *self.color, 1.0, None)
    }

    fn get_emission(&self, _light: &mut Light, _ray_in: &Vector3<f64>, _normal: &Ray3<f64>, _rng: &mut Rng) -> Option<&Color> {
        None
    }
}

pub struct Mix {
    factor: f64,
    pub a: MaterialBox,
    pub b: MaterialBox
}

impl Material for Mix {
    fn reflect(&self, light: &mut Light, ray_in: &Ray3<f64>, normal: &Ray3<f64>, rng: &mut Rng) -> Reflection {
        if self.factor < rng.next_f64() {
            self.a.reflect(light, ray_in, normal, rng)
        } else {
            self.b.reflect(light, ray_in, normal, rng)
        }
    }

    fn get_emission(&self, light: &mut Light, ray_in: &Vector3<f64>, normal: &Ray3<f64>, rng: &mut Rng) -> Option<&Color> {
        if self.factor < rng.next_f64() {
            self.a.get_emission(light, ray_in, normal, rng)
        } else {
            self.b.get_emission(light, ray_in, normal, rng)
        }
    }
}

struct FresnelMix {
    ior: f64,
    dispersion: f64,
    env_ior: f64,
    env_dispersion: f64,
    pub reflect: MaterialBox,
    pub refract: MaterialBox
}

impl Material for FresnelMix {
    fn reflect(&self, light: &mut Light, ray_in: &Ray3<f64>, normal: &Ray3<f64>, rng: &mut Rng) -> Reflection {
        if self.dispersion != 0.0 || self.env_dispersion != 0.0 {
            let wl = light.colored() * 0.001;
            let ior = self.ior + self.dispersion / (wl * wl);
            let env_ior = self.env_ior + self.env_dispersion / (wl * wl);
            let child = fresnel_mix(ior, env_ior, &self.reflect, &self.refract, &ray_in.direction, normal, rng);
            child.reflect(light, ray_in, normal, rng)
        } else {
            let child = fresnel_mix(self.ior, self.env_ior, &self.reflect, &self.refract, &ray_in.direction, normal, rng);
            child.reflect(light, ray_in, normal, rng)
        }
    }

    fn get_emission(&self, light: &mut Light, ray_in: &Vector3<f64>, normal: &Ray3<f64>, rng: &mut Rng) -> Option<&Color> {
        if self.dispersion != 0.0 || self.env_dispersion != 0.0 {
            let wl = light.colored() * 0.001;
            let ior = self.ior + self.dispersion / (wl * wl);
            let env_ior = self.env_ior + self.env_dispersion / (wl * wl);
            let child = fresnel_mix(ior, env_ior, &self.reflect, &self.refract, &ray_in, normal, rng);
            child.get_emission(light, ray_in, normal, rng)
        } else {
            let child = fresnel_mix(self.ior, self.env_ior, &self.reflect, &self.refract, ray_in, normal, rng);
            child.get_emission(light, ray_in, normal, rng)
        }
    }
}

fn fresnel_mix<'a>(ior: f64, env_ior: f64, reflect: &'a MaterialBox, refract: &'a MaterialBox, ray_in: &Vector3<f64>, normal: &Ray3<f64>, rng: &mut Rng) -> &'a MaterialBox {
    let factor = if ray_in.dot(&normal.direction) < 0.0 {
        math::utils::schlick(env_ior, ior, &normal.direction, ray_in)
    } else {
        math::utils::schlick(ior, env_ior, &-normal.direction, ray_in)
    };

    if factor > rng.next_f64() {
        reflect
    } else {
        refract
    }
}

struct Refractive {
    color: Box<Color>,
    ior: f64,
    dispersion: f64,
    env_ior: f64,
    env_dispersion: f64
}

impl Material for Refractive {
    fn reflect(&self, light: &mut Light, ray_in: &Ray3<f64>, normal: &Ray3<f64>, rng: &mut Rng) -> Reflection {
        if self.dispersion != 0.0 || self.env_dispersion != 0.0 {
            let wl = light.colored() * 0.001;
            let ior = self.ior + self.dispersion / (wl * wl);
            let env_ior = self.env_ior + self.env_dispersion / (wl * wl);
            refract(ior, env_ior, &self.color, ray_in, normal, rng)
        } else {
            refract(self.ior, self.env_ior, &self.color, ray_in, normal, rng)
        }
    }

    fn get_emission(&self, _light: &mut Light, _ray_in: &Vector3<f64>, _normal: &Ray3<f64>, _rng: &mut Rng) -> Option<&Color> {
        None
    }
}

fn refract<'a>(ior: f64, env_ior: f64, color: &'a Box<Color>, ray_in: &Ray3<f64>, normal: &Ray3<f64>, rng: &mut Rng) -> Reflection<'a> {
    let nl = if normal.direction.dot(&ray_in.direction) < 0.0 {
        normal.direction
    } else {
        -normal.direction
    };

    let reflected = ray_in.direction.sub_v(&normal.direction.mul_s(2.0 * normal.direction.dot(&ray_in.direction)));

    let into = normal.direction.dot(&nl) > 0.0;

    let nnt = if into { env_ior/ior } else { ior/env_ior };
    let ddn = ray_in.direction.dot(&nl);
    
    let cos2t = 1.0 - nnt * nnt * (1.0 - ddn * ddn);
    if cos2t < 0.0 { // Total internal reflection
        return Reflect(Ray::new(normal.origin, reflected), & **color, 1.0, None);
    }

    let s = if into { 1.0 } else { -1.0 }*(ddn * nnt + cos2t.sqrt());
    let tdir = ray_in.direction.mul_s(nnt).sub_v(&normal.direction.mul_s(s)).normalize();

    let a = ior - env_ior;
    let b = ior + env_ior;
    let r0 = a * a / (b * b);
    let c = 1.0 - if into { -ddn } else { tdir.dot(&normal.direction) };

    let re = r0 + (1.0 - r0) * c*c*c*c*c;
    let tr = 1.0 - re;
    let p = 0.25 + 0.5 * re;
    let rp = re / p;
    let tp = tr / (1.0 - p);

    if rng.next_f64() < p {
        return Reflect(Ray::new(normal.origin, reflected), & **color, rp, None);
    } else {
        return Reflect(Ray::new(normal.origin, tdir), & **color, tp, None);
    }
}




pub fn register_types(context: &mut Prelude) {
    let mut group = context.object("Material".into());
    {
        let mut object = group.object("Diffuse".into());
        object.add_decoder(decode_diffuse);
        object.arguments(vec!["color".into()]);
    }
    {
        let mut object = group.object("Emission".into());
        object.add_decoder(decode_emission);
        object.arguments(vec!["color".into()]);
    }
    {
        let mut object = group.object("Mirror".into());
        object.add_decoder(decode_mirror);
        object.arguments(vec!["color".into()]);
    }
    {
        let mut object = group.object("Mix".into());
        object.add_decoder(decode_mix);
        object.arguments(vec!["a".into(), "b".into(), "factor".into()]);
    }
    group.object("FresnelMix".into()).add_decoder(decode_fresnel_mix);
    group.object("Refractive".into()).add_decoder(decode_refractive);
}

pub fn decode_diffuse(entry: Entry) -> Result<(MaterialBox, bool), String> {
    let fields = try!(entry.as_object().ok_or("not an object".into()));

    let color = match fields.get("color") {
        Some(v) => try!(tracer::decode_parametric_number(v), "color"),
        None => return Err("missing field 'color'".into())
    };

    Ok((Box::new(Diffuse { color: color }) as MaterialBox, false))
}

pub fn decode_emission(entry: Entry) -> Result<(MaterialBox, bool), String> {
    let fields = try!(entry.as_object().ok_or("not an object".into()));

    let color = match fields.get("color") {
        Some(v) => try!(tracer::decode_parametric_number(v), "color"),
        None => return Err("missing field 'color'".into())
    };

    Ok((Box::new(Emission { color: color }) as MaterialBox, true))
}

pub fn decode_mirror(entry: Entry) -> Result<(MaterialBox, bool), String> {
    let fields = try!(entry.as_object().ok_or("not an object".into()));

    let color = match fields.get("color") {
        Some(v) => try!(tracer::decode_parametric_number(v), "color"),
        None => return Err("missing field 'color'".into())
    };

    Ok((Box::new(Mirror { color: color }) as MaterialBox, false))
}

pub fn decode_mix(entry: Entry) -> Result<(MaterialBox, bool), String> {
    let fields = try!(entry.as_object().ok_or("not an object".into()));

    let factor = match fields.get("factor") {
        Some(v) => try!(v.decode(), "factor"),
        None => return Err("missing field 'factor'".into())
    };

    let (a, a_emissive): (MaterialBox, bool) = match fields.get("a") {
        Some(v) => try!(v.dynamic_decode(), "a"),
        None => return Err("missing field 'a'".into())
    };

    let (b, b_emissive): (MaterialBox, bool) = match fields.get("b") {
        Some(v) => try!(v.dynamic_decode(), "b"),
        None => return Err("missing field 'b'".into())
    };

    Ok((Box::new(Mix {
        factor: factor,
        a: a,
        b: b
    }) as MaterialBox, a_emissive || b_emissive))
}

pub fn decode_fresnel_mix(entry: Entry) -> Result<(MaterialBox, bool), String> {
    let fields = try!(entry.as_object().ok_or("not an object".into()));

    let ior = match fields.get("ior") {
        Some(v) => try!(v.decode(), "ior"),
        None => return Err("missing field 'ior'".into())
    };

    let env_ior = match fields.get("env_ior") {
        Some(v) => try!(v.decode(), "env_ior"),
        None => 1.0
    };

    let dispersion = match fields.get("dispersion") {
        Some(v) => try!(v.decode(), "dispersion"),
        None => 0.0
    };

    let env_dispersion = match fields.get("env_dispersion") {
        Some(v) => try!(v.decode(), "env_dispersion"),
        None => 0.0
    };

    let (reflect, reflect_emissive): (MaterialBox, bool) = match fields.get("reflect") {
        Some(v) => try!(v.dynamic_decode(), "reflect"),
        None => return Err("missing field 'reflect'".into())
    };

    let (refract, refract_emissive): (MaterialBox, bool) = match fields.get("refract") {
        Some(v) => try!(v.dynamic_decode(), "refract"),
        None => return Err("missing field 'refract'".into())
    };

    Ok((Box::new(FresnelMix {
        ior: ior,
        dispersion: dispersion,
        env_ior: env_ior,
        env_dispersion: env_dispersion,
        reflect: reflect,
        refract: refract
    }) as MaterialBox, refract_emissive || reflect_emissive))
}

pub fn decode_refractive(entry: Entry) -> Result<(MaterialBox, bool), String> {
    let fields = try!(entry.as_object().ok_or("not an object".into()));

    let ior = match fields.get("ior") {
        Some(v) => try!(v.decode(), "ior"),
        None => return Err("missing field 'ior'".into())
    };

    let env_ior = match fields.get("env_ior") {
        Some(v) => try!(v.decode(), "env_ior"),
        None => 1.0
    };

    let dispersion = match fields.get("dispersion") {
        Some(v) => try!(v.decode(), "dispersion"),
        None => 0.0
    };

    let env_dispersion = match fields.get("env_dispersion") {
        Some(v) => try!(v.decode(), "env_dispersion"),
        None => 0.0
    };

    let color = match fields.get("color") {
        Some(v) => try!(tracer::decode_parametric_number(v), "color"),
        None => return Err("missing field 'color'".into())
    };

    Ok((Box::new(Refractive {
        ior: ior,
        dispersion: dispersion,
        env_ior: env_ior,
        env_dispersion: env_dispersion,
        color: color
    }) as MaterialBox, false))
}