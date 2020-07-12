use std::error::Error;

use rand::Rng;

use cgmath::{InnerSpace, Vector3};
use collision::Ray3;

use crate::{
    color::Light,
    math,
    project::{
        eval_context::{EvalContext, Evaluate, EvaluateOr},
        expressions::{Expressions, Vector},
        program::{ExecutionContext, Program, ProgramCompiler},
        SurfaceMaterial as ProjectMaterial,
    },
    shapes::Normal,
    tracer::{self, Emit, LightProgram, NormalInput, Reflect, Reflection, RenderContext},
};

pub(crate) struct Material<'p> {
    surface: SurfaceMaterial<'p>,
    normal_map: Option<Program<'p, NormalInput, Vector>>,
}

impl<'p> Material<'p> {
    pub fn from_project(
        project: crate::project::Material,
        eval_context: EvalContext,
        programs: ProgramCompiler<'p>,
        expressions: &Expressions,
    ) -> Result<Self, Box<dyn Error>> {
        let crate::project::Material {
            surface,
            normal_map,
        } = project;

        Ok(Material {
            surface: SurfaceMaterial::from_project(surface, eval_context, programs, expressions)?,
            normal_map: normal_map
                .map(|normal_map| programs.compile(&normal_map, expressions))
                .transpose()?,
        })
    }

    pub fn reflect(
        &self,
        light: &mut tracer::Light,
        ray_in: Ray3<f32>,
        normal: Ray3<f32>,
        rng: &mut impl Rng,
    ) -> Reflection<'_> {
        self.surface.reflect(light, ray_in, normal, rng)
    }

    pub fn get_emission(
        &self,
        light: &mut tracer::Light,
        ray_in: Vector3<f32>,
        normal: Ray3<f32>,
        rng: &mut impl Rng,
    ) -> Option<Program<RenderContext, Light>> {
        self.surface.get_emission(light, ray_in, normal, rng)
    }

    pub fn is_emissive(&self) -> bool {
        self.surface.is_emissive()
    }

    pub fn apply_normal_map(
        &self,
        normal: Normal,
        input: NormalInput,
        exe: &mut ExecutionContext<'p>,
    ) -> Vector3<f32> {
        if let Some(normal_map) = self.normal_map {
            let new_normal: Vector3<f32> = exe.run(normal_map, &input).into();
            normal.from_space(new_normal).normalize()
        } else {
            normal.vector()
        }
    }
}

pub(crate) enum SurfaceMaterial<'p> {
    Diffuse(Diffuse<'p>),
    Emission(Emission<'p>),
    Mirror(Mirror<'p>),
    Refractive(Refractive<'p>),
    Mix(Mix<'p>),
    FresnelMix(FresnelMix<'p>),
}

impl<'p> SurfaceMaterial<'p> {
    pub fn from_project(
        project: ProjectMaterial,
        eval_context: EvalContext,
        programs: ProgramCompiler<'p>,
        expressions: &Expressions,
    ) -> Result<Self, Box<dyn Error>> {
        Ok(match project {
            ProjectMaterial::Diffuse { color } => SurfaceMaterial::Diffuse(Diffuse {
                color: programs.compile(&color, expressions)?,
            }),
            ProjectMaterial::Emission { color } => SurfaceMaterial::Emission(Emission {
                color: programs.compile(&color, expressions)?,
            }),
            ProjectMaterial::Mirror { color } => SurfaceMaterial::Mirror(Mirror {
                color: programs.compile(&color, expressions)?,
            }),
            ProjectMaterial::Refractive {
                color,
                ior,
                env_ior,
                dispersion,
                env_dispersion,
            } => SurfaceMaterial::Refractive(Refractive {
                color: programs.compile(&color, expressions)?,
                ior: ior.evaluate(eval_context)?,
                env_ior: env_ior.evaluate_or(eval_context, 1.0)?,
                dispersion: dispersion.evaluate_or(eval_context, 0.0)?,
                env_dispersion: env_dispersion.evaluate_or(eval_context, 0.0)?,
            }),
            ProjectMaterial::Mix { amount, lhs, rhs } => SurfaceMaterial::Mix(Mix {
                factor: amount.evaluate(eval_context)?,
                a: Box::new(SurfaceMaterial::from_project(
                    *lhs,
                    eval_context,
                    programs,
                    expressions,
                )?),
                b: Box::new(SurfaceMaterial::from_project(
                    *rhs,
                    eval_context,
                    programs,
                    expressions,
                )?),
            }),
            ProjectMaterial::FresnelMix {
                ior,
                dispersion,
                env_ior,
                env_dispersion,
                reflect,
                refract,
            } => SurfaceMaterial::FresnelMix(FresnelMix {
                ior: ior.evaluate(eval_context)?,
                env_ior: env_ior.evaluate_or(eval_context, 1.0)?,
                dispersion: dispersion.evaluate_or(eval_context, 0.0)?,
                env_dispersion: env_dispersion.evaluate_or(eval_context, 0.0)?,
                reflect: Box::new(SurfaceMaterial::from_project(
                    *reflect,
                    eval_context,
                    programs,
                    expressions,
                )?),
                refract: Box::new(SurfaceMaterial::from_project(
                    *refract,
                    eval_context,
                    programs,
                    expressions,
                )?),
            }),
        })
    }

    pub fn reflect(
        &self,
        light: &mut tracer::Light,
        ray_in: Ray3<f32>,
        normal: Ray3<f32>,
        rng: &mut impl Rng,
    ) -> Reflection<'_> {
        match self {
            SurfaceMaterial::Diffuse(material) => material.reflect(ray_in, normal, rng),
            SurfaceMaterial::Emission(material) => material.reflect(),
            SurfaceMaterial::Mirror(material) => material.reflect(ray_in, normal),
            SurfaceMaterial::Refractive(material) => material.reflect(light, ray_in, normal, rng),
            SurfaceMaterial::Mix(material) => material.reflect(light, ray_in, normal, rng),
            SurfaceMaterial::FresnelMix(material) => material.reflect(light, ray_in, normal, rng),
        }
    }

    pub fn get_emission(
        &self,
        light: &mut tracer::Light,
        ray_in: Vector3<f32>,
        normal: Ray3<f32>,
        rng: &mut impl Rng,
    ) -> Option<Program<RenderContext, Light>> {
        match self {
            SurfaceMaterial::Emission(material) => material.get_emission(),
            SurfaceMaterial::Mix(material) => material.get_emission(light, ray_in, normal, rng),
            SurfaceMaterial::FresnelMix(material) => {
                material.get_emission(light, ray_in, normal, rng)
            }
            SurfaceMaterial::Diffuse(_)
            | SurfaceMaterial::Mirror(_)
            | SurfaceMaterial::Refractive(_) => None,
        }
    }

    pub fn is_emissive(&self) -> bool {
        match self {
            SurfaceMaterial::Emission(_) => true,
            SurfaceMaterial::Mix(material) => material.a.is_emissive() || material.b.is_emissive(),
            SurfaceMaterial::FresnelMix(material) => {
                material.reflect.is_emissive() || material.refract.is_emissive()
            }
            SurfaceMaterial::Diffuse(_)
            | SurfaceMaterial::Mirror(_)
            | SurfaceMaterial::Refractive(_) => false,
        }
    }
}

pub(crate) struct Diffuse<'p> {
    pub color: LightProgram<'p>,
}

impl<'p> Diffuse<'p> {
    fn reflect(&self, ray_in: Ray3<f32>, normal: Ray3<f32>, rng: &mut impl Rng) -> Reflection<'_> {
        let n = if ray_in.direction.dot(normal.direction) < 0.0 {
            normal.direction
        } else {
            -normal.direction
        };

        let reflected = math::utils::sample_hemisphere(rng, n);
        Reflect(
            Ray3::new(normal.origin, reflected),
            self.color,
            1.0,
            Some(lambertian),
        )
    }
}

fn lambertian(_ray_in: Vector3<f32>, ray_out: Vector3<f32>, normal: Vector3<f32>) -> f32 {
    2.0 * normal.dot(ray_out).abs()
}

pub(crate) struct Emission<'p> {
    pub color: LightProgram<'p>,
}

impl<'p> Emission<'p> {
    fn reflect(&self) -> Reflection<'_> {
        Emit(self.color)
    }

    fn get_emission(&self) -> Option<Program<RenderContext, Light>> {
        Some(self.color)
    }
}

pub(crate) struct Mirror<'p> {
    pub color: LightProgram<'p>,
}

impl<'p> Mirror<'p> {
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
            self.color,
            1.0,
            None,
        )
    }
}

pub(crate) struct Mix<'p> {
    factor: f32,
    pub a: Box<SurfaceMaterial<'p>>,
    pub b: Box<SurfaceMaterial<'p>>,
}

impl<'p> Mix<'p> {
    fn reflect(
        &self,
        light: &mut tracer::Light,
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
        light: &mut tracer::Light,
        ray_in: Vector3<f32>,
        normal: Ray3<f32>,
        rng: &mut impl Rng,
    ) -> Option<Program<RenderContext, Light>> {
        if self.factor < rng.gen() {
            self.a.get_emission(light, ray_in, normal, rng)
        } else {
            self.b.get_emission(light, ray_in, normal, rng)
        }
    }
}

pub(crate) struct FresnelMix<'p> {
    ior: f32,
    dispersion: f32,
    env_ior: f32,
    env_dispersion: f32,
    pub reflect: Box<SurfaceMaterial<'p>>,
    pub refract: Box<SurfaceMaterial<'p>>,
}

impl<'p> FresnelMix<'p> {
    fn reflect(
        &self,
        light: &mut tracer::Light,
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
        light: &mut tracer::Light,
        ray_in: Vector3<f32>,
        normal: Ray3<f32>,
        rng: &mut impl Rng,
    ) -> Option<Program<RenderContext, Light>> {
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
    reflect: &'a SurfaceMaterial<'a>,
    refract: &'a SurfaceMaterial<'a>,
    ray_in: Vector3<f32>,
    normal: Ray3<f32>,
    rng: &mut R,
) -> &'a SurfaceMaterial<'a> {
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

pub(crate) struct Refractive<'p> {
    color: LightProgram<'p>,
    ior: f32,
    dispersion: f32,
    env_ior: f32,
    env_dispersion: f32,
}

impl<'p> Refractive<'p> {
    fn reflect(
        &self,
        light: &mut tracer::Light,
        ray_in: Ray3<f32>,
        normal: Ray3<f32>,
        rng: &mut impl Rng,
    ) -> Reflection<'_> {
        if self.dispersion != 0.0 || self.env_dispersion != 0.0 {
            let wl = light.colored() * 0.001;
            let ior = self.ior + self.dispersion / (wl * wl);
            let env_ior = self.env_ior + self.env_dispersion / (wl * wl);
            refract(ior, env_ior, self.color, ray_in, normal, rng)
        } else {
            refract(self.ior, self.env_ior, self.color, ray_in, normal, rng)
        }
    }
}

fn refract<'a, R: Rng>(
    ior: f32,
    env_ior: f32,
    color: Program<'a, RenderContext, Light>,
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
        return Reflect(Ray3::new(normal.origin, reflected), color, 1.0, None);
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
        return Reflect(Ray3::new(normal.origin, reflected), color, rp, None);
    } else {
        return Reflect(Ray3::new(normal.origin, tdir), color, tp, None);
    }
}
