use rand::Rng;

use cgmath::{InnerSpace, Vector3};
use collision::Ray3;

use crate::tracer::{Emit, Light, Reflect, Reflection};

use crate::config::entry::Entry;
use crate::config::Prelude;

use crate::{
    color::{decode_color, Color},
    math,
};
use math::RenderMath;

pub enum Material {
    Diffuse(Diffuse),
    Emission(Emission),
    Mirror(Mirror),
    Refractive(Refractive),
    Mix(Mix),
    FresnelMix(FresnelMix),
}

impl Material {
    pub fn reflect(
        &self,
        light: &mut Light,
        ray_in: Ray3<f32>,
        normal: Ray3<f32>,
        rng: &mut impl Rng,
    ) -> Reflection<'_> {
        match self {
            Material::Diffuse(material) => material.reflect(ray_in, normal, rng),
            Material::Emission(material) => material.reflect(),
            Material::Mirror(material) => material.reflect(ray_in, normal),
            Material::Refractive(material) => material.reflect(light, ray_in, normal, rng),
            Material::Mix(material) => material.reflect(light, ray_in, normal, rng),
            Material::FresnelMix(material) => material.reflect(light, ray_in, normal, rng),
        }
    }

    pub fn get_emission(
        &self,
        light: &mut Light,
        ray_in: Vector3<f32>,
        normal: Ray3<f32>,
        rng: &mut impl Rng,
    ) -> Option<&RenderMath<Color>> {
        match self {
            Material::Emission(material) => material.get_emission(),
            Material::Mix(material) => material.get_emission(light, ray_in, normal, rng),
            Material::FresnelMix(material) => material.get_emission(light, ray_in, normal, rng),
            Material::Diffuse(_) | Material::Mirror(_) | Material::Refractive(_) => None,
        }
    }
}

pub struct Diffuse {
    pub color: RenderMath<Color>,
}

impl Diffuse {
    fn reflect(&self, ray_in: Ray3<f32>, normal: Ray3<f32>, rng: &mut impl Rng) -> Reflection<'_> {
        let n = if ray_in.direction.dot(normal.direction) < 0.0 {
            normal.direction
        } else {
            -normal.direction
        };

        let reflected = math::utils::sample_hemisphere(rng, n);
        Reflect(
            Ray3::new(normal.origin, reflected),
            &self.color,
            1.0,
            Some(lambertian),
        )
    }
}

fn lambertian(_ray_in: Vector3<f32>, ray_out: Vector3<f32>, normal: Vector3<f32>) -> f32 {
    2.0 * normal.dot(ray_out).abs()
}

pub struct Emission {
    pub color: RenderMath<Color>,
}

impl Emission {
    fn reflect(&self) -> Reflection<'_> {
        Emit(&self.color)
    }

    fn get_emission(&self) -> Option<&RenderMath<Color>> {
        Some(&self.color)
    }
}

pub struct Mirror {
    pub color: RenderMath<Color>,
}

impl Mirror {
    fn reflect(&self, ray_in: Ray3<f32>, normal: Ray3<f32>) -> Reflection<'_> {
        let mut n = if ray_in.direction.dot(normal.direction) < 0.0 {
            normal.direction
        } else {
            -normal.direction
        };

        let perp = ray_in.direction.dot(n) * 2.0;
        n *= perp;
        Reflect(
            Ray3::new(normal.origin, ray_in.direction - n),
            &self.color,
            1.0,
            None,
        )
    }
}

pub struct Mix {
    factor: f32,
    pub a: Box<Material>,
    pub b: Box<Material>,
}

impl Mix {
    fn reflect(
        &self,
        light: &mut Light,
        ray_in: Ray3<f32>,
        normal: Ray3<f32>,
        rng: &mut impl Rng,
    ) -> Reflection<'_> {
        if self.factor < rng.gen() {
            self.a.reflect(light, ray_in, normal, rng)
        } else {
            self.b.reflect(light, ray_in, normal, rng)
        }
    }

    fn get_emission(
        &self,
        light: &mut Light,
        ray_in: Vector3<f32>,
        normal: Ray3<f32>,
        rng: &mut impl Rng,
    ) -> Option<&RenderMath<Color>> {
        if self.factor < rng.gen() {
            self.a.get_emission(light, ray_in, normal, rng)
        } else {
            self.b.get_emission(light, ray_in, normal, rng)
        }
    }
}

pub struct FresnelMix {
    ior: f32,
    dispersion: f32,
    env_ior: f32,
    env_dispersion: f32,
    pub reflect: Box<Material>,
    pub refract: Box<Material>,
}

impl FresnelMix {
    fn reflect(
        &self,
        light: &mut Light,
        ray_in: Ray3<f32>,
        normal: Ray3<f32>,
        rng: &mut impl Rng,
    ) -> Reflection<'_> {
        if self.dispersion != 0.0 || self.env_dispersion != 0.0 {
            let wl = light.colored() * 0.001;
            let ior = self.ior + self.dispersion / (wl * wl);
            let env_ior = self.env_ior + self.env_dispersion / (wl * wl);
            let child = fresnel_mix(
                ior,
                env_ior,
                &self.reflect,
                &self.refract,
                ray_in.direction,
                normal,
                rng,
            );
            child.reflect(light, ray_in, normal, rng)
        } else {
            let child = fresnel_mix(
                self.ior,
                self.env_ior,
                &self.reflect,
                &self.refract,
                ray_in.direction,
                normal,
                rng,
            );
            child.reflect(light, ray_in, normal, rng)
        }
    }

    fn get_emission(
        &self,
        light: &mut Light,
        ray_in: Vector3<f32>,
        normal: Ray3<f32>,
        rng: &mut impl Rng,
    ) -> Option<&RenderMath<Color>> {
        if self.dispersion != 0.0 || self.env_dispersion != 0.0 {
            let wl = light.colored() * 0.001;
            let ior = self.ior + self.dispersion / (wl * wl);
            let env_ior = self.env_ior + self.env_dispersion / (wl * wl);
            let child = fresnel_mix(
                ior,
                env_ior,
                &self.reflect,
                &self.refract,
                ray_in,
                normal,
                rng,
            );
            child.get_emission(light, ray_in, normal, rng)
        } else {
            let child = fresnel_mix(
                self.ior,
                self.env_ior,
                &self.reflect,
                &self.refract,
                ray_in,
                normal,
                rng,
            );
            child.get_emission(light, ray_in, normal, rng)
        }
    }
}

fn fresnel_mix<'a, R: Rng>(
    ior: f32,
    env_ior: f32,
    reflect: &'a Material,
    refract: &'a Material,
    ray_in: Vector3<f32>,
    normal: Ray3<f32>,
    rng: &mut R,
) -> &'a Material {
    let factor = if ray_in.dot(normal.direction) < 0.0 {
        math::utils::schlick(env_ior, ior, normal.direction, ray_in)
    } else {
        math::utils::schlick(ior, env_ior, -normal.direction, ray_in)
    };

    if factor > rng.gen() {
        reflect
    } else {
        refract
    }
}

pub struct Refractive {
    color: RenderMath<Color>,
    ior: f32,
    dispersion: f32,
    env_ior: f32,
    env_dispersion: f32,
}

impl Refractive {
    fn reflect(
        &self,
        light: &mut Light,
        ray_in: Ray3<f32>,
        normal: Ray3<f32>,
        rng: &mut impl Rng,
    ) -> Reflection<'_> {
        if self.dispersion != 0.0 || self.env_dispersion != 0.0 {
            let wl = light.colored() * 0.001;
            let ior = self.ior + self.dispersion / (wl * wl);
            let env_ior = self.env_ior + self.env_dispersion / (wl * wl);
            refract(ior, env_ior, &self.color, ray_in, normal, rng)
        } else {
            refract(self.ior, self.env_ior, &self.color, ray_in, normal, rng)
        }
    }
}

fn refract<'a, R: Rng>(
    ior: f32,
    env_ior: f32,
    color: &'a RenderMath<Color>,
    ray_in: Ray3<f32>,
    normal: Ray3<f32>,
    rng: &mut R,
) -> Reflection<'a> {
    let nl = if normal.direction.dot(ray_in.direction) < 0.0 {
        normal.direction
    } else {
        -normal.direction
    };

    let reflected =
        ray_in.direction - (normal.direction * 2.0 * normal.direction.dot(ray_in.direction));

    let into = normal.direction.dot(nl) > 0.0;

    let nnt = if into { env_ior / ior } else { ior / env_ior };
    let ddn = ray_in.direction.dot(nl);

    let cos2t = 1.0 - nnt * nnt * (1.0 - ddn * ddn);
    if cos2t < 0.0 {
        // Total internal reflection
        return Reflect(Ray3::new(normal.origin, reflected), &color, 1.0, None);
    }

    let s = if into { 1.0 } else { -1.0 } * (ddn * nnt + cos2t.sqrt());
    let tdir = (ray_in.direction * nnt - normal.direction * s).normalize();

    let a = ior - env_ior;
    let b = ior + env_ior;
    let r0 = a * a / (b * b);
    let c = 1.0
        - if into {
            -ddn
        } else {
            tdir.dot(normal.direction)
        };

    let re = r0 + (1.0 - r0) * c * c * c * c * c;
    let tr = 1.0 - re;
    let p = 0.25 + 0.5 * re;
    let rp = re / p;
    let tp = tr / (1.0 - p);

    if rng.gen::<f32>() < p {
        return Reflect(Ray3::new(normal.origin, reflected), &color, rp, None);
    } else {
        return Reflect(Ray3::new(normal.origin, tdir), &color, tp, None);
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
    group
        .object("FresnelMix".into())
        .add_decoder(decode_fresnel_mix);
    group
        .object("Refractive".into())
        .add_decoder(decode_refractive);
}

pub fn decode_diffuse(entry: Entry<'_>) -> Result<(Material, bool), String> {
    let fields = entry.as_object().ok_or("not an object")?;

    let color = match fields.get("color") {
        Some(v) => try_for!(decode_color(v), "color"),
        None => return Err("missing field 'color'".into()),
    };

    Ok((Material::Diffuse(Diffuse { color }), false))
}

pub fn decode_emission(entry: Entry<'_>) -> Result<(Material, bool), String> {
    let fields = entry.as_object().ok_or("not an object")?;

    let color = match fields.get("color") {
        Some(v) => try_for!(decode_color(v), "color"),
        None => return Err("missing field 'color'".into()),
    };

    Ok((Material::Emission(Emission { color }), true))
}

pub fn decode_mirror(entry: Entry<'_>) -> Result<(Material, bool), String> {
    let fields = entry.as_object().ok_or("not an object")?;

    let color = match fields.get("color") {
        Some(v) => try_for!(decode_color(v), "color"),
        None => return Err("missing field 'color'".into()),
    };

    Ok((Material::Mirror(Mirror { color }), false))
}

pub fn decode_mix(entry: Entry<'_>) -> Result<(Material, bool), String> {
    let fields = entry.as_object().ok_or("not an object")?;

    let factor = match fields.get("factor") {
        Some(v) => try_for!(v.decode(), "factor"),
        None => return Err("missing field 'factor'".into()),
    };

    let (a, a_emissive): (Material, bool) = match fields.get("a") {
        Some(v) => try_for!(v.dynamic_decode(), "a"),
        None => return Err("missing field 'a'".into()),
    };

    let (b, b_emissive): (Material, bool) = match fields.get("b") {
        Some(v) => try_for!(v.dynamic_decode(), "b"),
        None => return Err("missing field 'b'".into()),
    };

    Ok((
        Material::Mix(Mix {
            factor,
            a: Box::new(a),
            b: Box::new(b),
        }),
        a_emissive || b_emissive,
    ))
}

pub fn decode_fresnel_mix(entry: Entry<'_>) -> Result<(Material, bool), String> {
    let fields = entry.as_object().ok_or("not an object")?;

    let ior = match fields.get("ior") {
        Some(v) => try_for!(v.decode(), "ior"),
        None => return Err("missing field 'ior'".into()),
    };

    let env_ior = match fields.get("env_ior") {
        Some(v) => try_for!(v.decode(), "env_ior"),
        None => 1.0,
    };

    let dispersion = match fields.get("dispersion") {
        Some(v) => try_for!(v.decode(), "dispersion"),
        None => 0.0,
    };

    let env_dispersion = match fields.get("env_dispersion") {
        Some(v) => try_for!(v.decode(), "env_dispersion"),
        None => 0.0,
    };

    let (reflect, reflect_emissive): (Material, bool) = match fields.get("reflect") {
        Some(v) => try_for!(v.dynamic_decode(), "reflect"),
        None => return Err("missing field 'reflect'".into()),
    };

    let (refract, refract_emissive): (Material, bool) = match fields.get("refract") {
        Some(v) => try_for!(v.dynamic_decode(), "refract"),
        None => return Err("missing field 'refract'".into()),
    };

    Ok((
        Material::FresnelMix(FresnelMix {
            ior,
            dispersion,
            env_ior,
            env_dispersion,
            reflect: Box::new(reflect),
            refract: Box::new(refract),
        }),
        refract_emissive || reflect_emissive,
    ))
}

pub fn decode_refractive(entry: Entry<'_>) -> Result<(Material, bool), String> {
    let fields = entry.as_object().ok_or("not an object")?;

    let ior = match fields.get("ior") {
        Some(v) => try_for!(v.decode(), "ior"),
        None => return Err("missing field 'ior'".into()),
    };

    let env_ior = match fields.get("env_ior") {
        Some(v) => try_for!(v.decode(), "env_ior"),
        None => 1.0,
    };

    let dispersion = match fields.get("dispersion") {
        Some(v) => try_for!(v.decode(), "dispersion"),
        None => 0.0,
    };

    let env_dispersion = match fields.get("env_dispersion") {
        Some(v) => try_for!(v.decode(), "env_dispersion"),
        None => 0.0,
    };

    let color = match fields.get("color") {
        Some(v) => try_for!(decode_color(v), "color"),
        None => return Err("missing field 'color'".into()),
    };

    Ok((
        Material::Refractive(Refractive {
            ior,
            dispersion,
            env_ior,
            env_dispersion,
            color,
        }),
        false,
    ))
}
