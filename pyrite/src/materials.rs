use rand::Rng;

use cgmath::{InnerSpace, Vector3};
use collision::Ray3;

use crate::tracer::{self, Color, Emit, Light, Material, Reflect, Reflection};

use crate::config::entry::Entry;
use crate::config::Prelude;

use crate::math;

pub type MaterialBox<R> = Box<dyn Material<R> + 'static + Send + Sync>;

pub struct Diffuse {
    pub color: Box<Color>,
}

impl<R: Rng> Material<R> for Diffuse {
    fn reflect(
        &self,
        _light: &mut Light,
        ray_in: Ray3<f64>,
        normal: Ray3<f64>,
        rng: &mut R,
    ) -> Reflection<'_> {
        let n = if ray_in.direction.dot(normal.direction) < 0.0 {
            normal.direction
        } else {
            -normal.direction
        };

        let reflected = math::utils::sample_hemisphere(rng, n);
        Reflect(
            Ray3::new(normal.origin, reflected),
            &*self.color,
            1.0,
            Some(lambertian),
        )
    }

    fn get_emission(
        &self,
        _light: &mut Light,
        _ray_in: Vector3<f64>,
        _normal: Ray3<f64>,
        _rng: &mut R,
    ) -> Option<&Color> {
        None
    }
}

fn lambertian(_ray_in: Vector3<f64>, ray_out: Vector3<f64>, normal: Vector3<f64>) -> f64 {
    2.0 * normal.dot(ray_out).abs()
}

pub struct Emission {
    pub color: Box<Color>,
}

impl<R: Rng> Material<R> for Emission {
    fn reflect(
        &self,
        _light: &mut Light,
        _ray_in: Ray3<f64>,
        _normal: Ray3<f64>,
        _rng: &mut R,
    ) -> Reflection<'_> {
        Emit(&*self.color)
    }

    fn get_emission(
        &self,
        _light: &mut Light,
        _ray_in: Vector3<f64>,
        _normal: Ray3<f64>,
        _rng: &mut R,
    ) -> Option<&Color> {
        Some(&*self.color)
    }
}

pub struct Mirror {
    pub color: Box<Color>,
}

impl<R: Rng> Material<R> for Mirror {
    fn reflect(
        &self,
        _light: &mut Light,
        ray_in: Ray3<f64>,
        normal: Ray3<f64>,
        _rng: &mut R,
    ) -> Reflection<'_> {
        let mut n = if ray_in.direction.dot(normal.direction) < 0.0 {
            normal.direction
        } else {
            -normal.direction
        };

        let perp = ray_in.direction.dot(n) * 2.0;
        n *= perp;
        Reflect(
            Ray3::new(normal.origin, ray_in.direction - n),
            &*self.color,
            1.0,
            None,
        )
    }

    fn get_emission(
        &self,
        _light: &mut Light,
        _ray_in: Vector3<f64>,
        _normal: Ray3<f64>,
        _rng: &mut R,
    ) -> Option<&Color> {
        None
    }
}

pub struct Mix<R: Rng> {
    factor: f64,
    pub a: MaterialBox<R>,
    pub b: MaterialBox<R>,
}

impl<R: Rng> Material<R> for Mix<R> {
    fn reflect(
        &self,
        light: &mut Light,
        ray_in: Ray3<f64>,
        normal: Ray3<f64>,
        rng: &mut R,
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
        ray_in: Vector3<f64>,
        normal: Ray3<f64>,
        rng: &mut R,
    ) -> Option<&Color> {
        if self.factor < rng.gen() {
            self.a.get_emission(light, ray_in, normal, rng)
        } else {
            self.b.get_emission(light, ray_in, normal, rng)
        }
    }
}

struct FresnelMix<R: Rng> {
    ior: f64,
    dispersion: f64,
    env_ior: f64,
    env_dispersion: f64,
    pub reflect: MaterialBox<R>,
    pub refract: MaterialBox<R>,
}

impl<R: Rng> Material<R> for FresnelMix<R> {
    fn reflect(
        &self,
        light: &mut Light,
        ray_in: Ray3<f64>,
        normal: Ray3<f64>,
        rng: &mut R,
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
        ray_in: Vector3<f64>,
        normal: Ray3<f64>,
        rng: &mut R,
    ) -> Option<&Color> {
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
    ior: f64,
    env_ior: f64,
    reflect: &'a MaterialBox<R>,
    refract: &'a MaterialBox<R>,
    ray_in: Vector3<f64>,
    normal: Ray3<f64>,
    rng: &mut R,
) -> &'a MaterialBox<R> {
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

struct Refractive {
    color: Box<Color>,
    ior: f64,
    dispersion: f64,
    env_ior: f64,
    env_dispersion: f64,
}

impl<R: Rng> Material<R> for Refractive {
    fn reflect(
        &self,
        light: &mut Light,
        ray_in: Ray3<f64>,
        normal: Ray3<f64>,
        rng: &mut R,
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

    fn get_emission(
        &self,
        _light: &mut Light,
        _ray_in: Vector3<f64>,
        _normal: Ray3<f64>,
        _rng: &mut R,
    ) -> Option<&Color> {
        None
    }
}

fn refract<'a, R: Rng>(
    ior: f64,
    env_ior: f64,
    color: &'a Box<Color>,
    ray_in: Ray3<f64>,
    normal: Ray3<f64>,
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
        return Reflect(Ray3::new(normal.origin, reflected), &**color, 1.0, None);
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

    if rng.gen::<f64>() < p {
        return Reflect(Ray3::new(normal.origin, reflected), &**color, rp, None);
    } else {
        return Reflect(Ray3::new(normal.origin, tdir), &**color, tp, None);
    }
}

pub fn register_types<R: Rng + 'static>(context: &mut Prelude) {
    let mut group = context.object("Material".into());
    {
        let mut object = group.object("Diffuse".into());
        object.add_decoder(decode_diffuse::<R>);
        object.arguments(vec!["color".into()]);
    }
    {
        let mut object = group.object("Emission".into());
        object.add_decoder(decode_emission::<R>);
        object.arguments(vec!["color".into()]);
    }
    {
        let mut object = group.object("Mirror".into());
        object.add_decoder(decode_mirror::<R>);
        object.arguments(vec!["color".into()]);
    }
    {
        let mut object = group.object("Mix".into());
        object.add_decoder(decode_mix::<R>);
        object.arguments(vec!["a".into(), "b".into(), "factor".into()]);
    }
    group
        .object("FresnelMix".into())
        .add_decoder(decode_fresnel_mix::<R>);
    group
        .object("Refractive".into())
        .add_decoder(decode_refractive::<R>);
}

pub fn decode_diffuse<R: Rng>(entry: Entry<'_>) -> Result<(MaterialBox<R>, bool), String> {
    let fields = entry.as_object().ok_or("not an object")?;

    let color = match fields.get("color") {
        Some(v) => try_for!(tracer::decode_parametric_number(v), "color"),
        None => return Err("missing field 'color'".into()),
    };

    Ok((Box::new(Diffuse { color: color }) as MaterialBox<R>, false))
}

pub fn decode_emission<R: Rng>(entry: Entry<'_>) -> Result<(MaterialBox<R>, bool), String> {
    let fields = entry.as_object().ok_or("not an object")?;

    let color = match fields.get("color") {
        Some(v) => try_for!(tracer::decode_parametric_number(v), "color"),
        None => return Err("missing field 'color'".into()),
    };

    Ok((Box::new(Emission { color: color }) as MaterialBox<R>, true))
}

pub fn decode_mirror<R: Rng>(entry: Entry<'_>) -> Result<(MaterialBox<R>, bool), String> {
    let fields = entry.as_object().ok_or("not an object")?;

    let color = match fields.get("color") {
        Some(v) => try_for!(tracer::decode_parametric_number(v), "color"),
        None => return Err("missing field 'color'".into()),
    };

    Ok((Box::new(Mirror { color: color }) as MaterialBox<R>, false))
}

pub fn decode_mix<R: Rng + 'static>(entry: Entry<'_>) -> Result<(MaterialBox<R>, bool), String> {
    let fields = entry.as_object().ok_or("not an object")?;

    let factor = match fields.get("factor") {
        Some(v) => try_for!(v.decode(), "factor"),
        None => return Err("missing field 'factor'".into()),
    };

    let (a, a_emissive): (MaterialBox<R>, bool) = match fields.get("a") {
        Some(v) => try_for!(v.dynamic_decode(), "a"),
        None => return Err("missing field 'a'".into()),
    };

    let (b, b_emissive): (MaterialBox<R>, bool) = match fields.get("b") {
        Some(v) => try_for!(v.dynamic_decode(), "b"),
        None => return Err("missing field 'b'".into()),
    };

    Ok((
        Box::new(Mix {
            factor: factor,
            a: a,
            b: b,
        }) as MaterialBox<R>,
        a_emissive || b_emissive,
    ))
}

pub fn decode_fresnel_mix<R: Rng + 'static>(
    entry: Entry<'_>,
) -> Result<(MaterialBox<R>, bool), String> {
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

    let (reflect, reflect_emissive): (MaterialBox<R>, bool) = match fields.get("reflect") {
        Some(v) => try_for!(v.dynamic_decode(), "reflect"),
        None => return Err("missing field 'reflect'".into()),
    };

    let (refract, refract_emissive): (MaterialBox<R>, bool) = match fields.get("refract") {
        Some(v) => try_for!(v.dynamic_decode(), "refract"),
        None => return Err("missing field 'refract'".into()),
    };

    Ok((
        Box::new(FresnelMix {
            ior: ior,
            dispersion: dispersion,
            env_ior: env_ior,
            env_dispersion: env_dispersion,
            reflect: reflect,
            refract: refract,
        }) as MaterialBox<R>,
        refract_emissive || reflect_emissive,
    ))
}

pub fn decode_refractive<R: Rng>(entry: Entry<'_>) -> Result<(MaterialBox<R>, bool), String> {
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
        Some(v) => try_for!(tracer::decode_parametric_number(v), "color"),
        None => return Err("missing field 'color'".into()),
    };

    Ok((
        Box::new(Refractive {
            ior: ior,
            dispersion: dispersion,
            env_ior: env_ior,
            env_dispersion: env_dispersion,
            color: color,
        }) as MaterialBox<R>,
        false,
    ))
}
