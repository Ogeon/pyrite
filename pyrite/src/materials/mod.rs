use std::{borrow::Cow, cell::Cell, convert::TryFrom, error::Error};

use cgmath::{InnerSpace, Point2, Vector3};
use typed_nodes::Key;

use crate::{
    program::{
        ExecutionContext, NumberInput, Program, ProgramCompiler, ProgramFor, ProgramInput,
        VectorInput,
    },
    project::{
        eval_context::{EvalContext, Evaluate, EvaluateOr},
        expressions::{self, Expression, Vector},
        materials::{BinaryOperator, SurfaceMaterial as MaterialNode},
        Nodes,
    },
    shapes::Normal,
    tracer::{Brdf, LightProgram, NormalInput},
};
use rand::{prelude::SliceRandom, Rng};

mod diffuse;
mod mirror;
mod refractive;

#[derive(Copy, Clone)]
pub(crate) struct Material<'a> {
    surface: SurfaceMaterial<'a>,
    normal_map: Option<ProgramFor<'a, NormalInput, Vector>>,
}

impl<'a> Material<'a> {
    pub(crate) fn from_project(
        material: crate::project::Material,
        programs: ProgramCompiler<'a>,
        nodes: &mut Nodes,
        allocator: &'a bumpalo::Bump,
    ) -> Result<Self, Box<dyn Error>> {
        Ok(Material {
            surface: SurfaceMaterial::from_project(material.surface, programs, nodes, allocator)?,
            normal_map: material
                .normal_map
                .map(|program| programs.compile(&program, nodes))
                .transpose()?,
        })
    }

    pub(crate) fn choose_component(&self, rng: &mut impl Rng) -> MaterialComponent<'a> {
        self.surface
            .components
            .choose(rng)
            .cloned()
            .expect("there should be at least one component")
    }

    pub(crate) fn choose_emissive(&self, rng: &mut impl Rng) -> MaterialComponent<'a> {
        self.surface
            .emissive
            .choose(rng)
            .cloned()
            .expect("the material is not emissive")
    }

    pub(crate) fn is_emissive(&self) -> bool {
        !self.surface.emissive.is_empty()
    }

    pub fn apply_normal_map(
        &self,
        normal: Normal,
        input: NormalInput,
        exe: &mut ExecutionContext<'a>,
    ) -> Vector3<f32> {
        if let Some(normal_map) = self.normal_map {
            let new_normal: Vector3<f32> = exe.run(normal_map, &input).into();
            normal.from_space(new_normal).normalize()
        } else {
            normal.vector()
        }
    }
}

#[derive(Copy, Clone)]
pub(crate) struct SurfaceMaterial<'a> {
    components: &'a [MaterialComponent<'a>],
    emissive: &'a [MaterialComponent<'a>],
}

impl<'a> SurfaceMaterial<'a> {
    pub(crate) fn from_project(
        material: Key<MaterialNode>,
        programs: ProgramCompiler<'a>,
        nodes: &mut Nodes,
        allocator: &'a bumpalo::Bump,
    ) -> Result<Self, Box<dyn Error>> {
        struct StackEntry {
            material: Key<MaterialNode>,
            probability: Option<Expression>,
        }

        let mut stack = vec![StackEntry {
            material,
            probability: None,
        }];

        let mut components = Vec::new();
        let mut emissive = Vec::new();

        while let Some(entry) = stack.pop() {
            match nodes.get(entry.material).expect("missing material") {
                MaterialNode::Emissive { color } => {
                    let component = MaterialComponent {
                        selection_compensation: 1.0,
                        probability: entry
                            .probability
                            .map(|expression| programs.compile(&expression, nodes))
                            .transpose()?,
                        bsdf: SurfaceBsdf {
                            color: programs.compile(color, nodes)?,
                            bsdf_type: SurfaceBsdfType::Emissive,
                        },
                    };
                    components.push(component);
                    emissive.push(component);
                }
                MaterialNode::Diffuse { color } => components.push(MaterialComponent {
                    selection_compensation: 1.0,
                    probability: entry
                        .probability
                        .map(|expression| programs.compile(&expression, nodes))
                        .transpose()?,
                    bsdf: SurfaceBsdf {
                        color: programs.compile(color, nodes)?,
                        bsdf_type: SurfaceBsdfType::Diffuse,
                    },
                }),
                MaterialNode::Mirror { color } => components.push(MaterialComponent {
                    selection_compensation: 1.0,
                    probability: entry
                        .probability
                        .map(|expression| programs.compile(&expression, nodes))
                        .transpose()?,
                    bsdf: SurfaceBsdf {
                        color: programs.compile(color, nodes)?,
                        bsdf_type: SurfaceBsdfType::Mirror,
                    },
                }),
                MaterialNode::Refractive {
                    color,
                    ior,
                    dispersion,
                    env_ior,
                    env_dispersion,
                } => {
                    let eval_context = EvalContext { nodes };
                    components.push(MaterialComponent {
                        selection_compensation: 1.0,
                        probability: entry
                            .probability
                            .map(|expression| programs.compile(&expression, nodes))
                            .transpose()?,
                        bsdf: SurfaceBsdf {
                            color: programs.compile(color, nodes)?,
                            bsdf_type: SurfaceBsdfType::Refractive {
                                properties: refractive::Properties {
                                    ior: ior.evaluate(eval_context)?,
                                    env_ior: env_ior.evaluate_or(eval_context, 1.0)?,
                                    dispersion: dispersion.evaluate_or(eval_context, 0.0)?,
                                    env_dispersion: env_dispersion
                                        .evaluate_or(eval_context, 0.0)?,
                                },
                            },
                        },
                    })
                }
                &MaterialNode::Mix { lhs, rhs, amount } => {
                    let amount = expressions::insert_clamp(nodes, amount, 0.0.into(), 1.0.into());
                    let lhs_probability = match entry.probability {
                        Some(probability) => expressions::insert_mul(nodes, probability, amount),
                        None => amount,
                    };

                    stack.push(StackEntry {
                        material: lhs,
                        probability: Some(lhs_probability),
                    });
                    stack.push(StackEntry {
                        material: rhs,
                        probability: Some(expressions::insert_sub(
                            nodes,
                            1.0.into(),
                            lhs_probability,
                        )),
                    });
                }
                &MaterialNode::Binary {
                    operator: BinaryOperator::Add,
                    lhs,
                    rhs,
                } => {
                    stack.push(StackEntry {
                        material: lhs,
                        probability: entry.probability,
                    });
                    stack.push(StackEntry {
                        material: rhs,
                        probability: entry.probability,
                    });
                }
            }
        }

        let selection_compensation = components.len() as f32;
        for component in &mut components {
            component.selection_compensation = selection_compensation;
        }

        let selection_compensation = emissive.len() as f32;
        for component in &mut emissive {
            component.selection_compensation = selection_compensation;
        }

        Ok(SurfaceMaterial {
            components: allocator.alloc_slice_copy(&components),
            emissive: allocator.alloc_slice_copy(&emissive),
        })
    }
}

#[derive(Copy, Clone)]
pub(crate) struct MaterialComponent<'a> {
    selection_compensation: f32,
    probability: Option<Program<'a, ProbabilityNumberInput, ProbabilityVectorInput, f32>>,
    pub(crate) bsdf: SurfaceBsdf<'a>,
}

impl<'a> MaterialComponent<'a> {
    pub(crate) fn get_probability(
        &self,
        exe: &mut ExecutionContext<'a>,
        input: &ProbabilityInput,
    ) -> f32 {
        if let Some(program) = self.probability {
            exe.run(program, input) * self.selection_compensation
        } else {
            self.selection_compensation
        }
    }
}

pub(crate) struct ProbabilityInput {
    pub(crate) wavelength: f32,
    pub(crate) wavelength_used: Cell<bool>,
    pub(crate) normal: Vector3<f32>,
    pub(crate) incident: Vector3<f32>,
    pub(crate) texture_coordinate: Point2<f32>,
}

impl ProgramInput for ProbabilityInput {
    type NumberInput = ProbabilityNumberInput;
    type VectorInput = ProbabilityVectorInput;

    fn get_number_input(&self, input: Self::NumberInput) -> f32 {
        match input {
            ProbabilityNumberInput::Wavelength => {
                self.wavelength_used.set(true);
                self.wavelength
            }
        }
    }

    fn get_vector_input(&self, input: Self::VectorInput) -> Vector {
        match input {
            ProbabilityVectorInput::Normal => self.normal.into(),
            ProbabilityVectorInput::Incident => self.incident.into(),
            ProbabilityVectorInput::TextureCoordinates => self.texture_coordinate.into(),
        }
    }
}

#[derive(Copy, Clone)]
pub(crate) enum ProbabilityNumberInput {
    Wavelength,
}

impl TryFrom<NumberInput> for ProbabilityNumberInput {
    type Error = Cow<'static, str>;

    fn try_from(value: NumberInput) -> Result<Self, Self::Error> {
        match value {
            NumberInput::Wavelength => Ok(ProbabilityNumberInput::Wavelength),
        }
    }
}

#[derive(Copy, Clone)]
pub(crate) enum ProbabilityVectorInput {
    Normal,
    Incident,
    TextureCoordinates,
}

impl TryFrom<VectorInput> for ProbabilityVectorInput {
    type Error = Cow<'static, str>;

    fn try_from(value: VectorInput) -> Result<Self, Self::Error> {
        match value {
            VectorInput::Normal => Ok(ProbabilityVectorInput::Normal),
            VectorInput::Incident => Ok(ProbabilityVectorInput::Incident),
            VectorInput::TextureCoordinates => Ok(ProbabilityVectorInput::TextureCoordinates),
        }
    }
}

#[derive(Copy, Clone)]
pub(crate) struct SurfaceBsdf<'a> {
    pub color: LightProgram<'a>,
    bsdf_type: SurfaceBsdfType,
}

impl<'a> SurfaceBsdf<'a> {
    pub(crate) fn scatter(
        &self,
        in_direction: Vector3<f32>,
        normal: Vector3<f32>,
        wavelength: f32,
        rng: &mut impl Rng,
    ) -> Scattering {
        self.bsdf_type
            .scatter(in_direction, normal, wavelength, rng)
    }
}

#[derive(Copy, Clone)]
enum SurfaceBsdfType {
    Emissive,
    Diffuse,
    Mirror,
    Refractive { properties: refractive::Properties },
}

impl SurfaceBsdfType {
    pub(crate) fn scatter(
        &self,
        in_direction: Vector3<f32>,
        normal: Vector3<f32>,
        wavelength: f32,
        rng: &mut impl Rng,
    ) -> Scattering {
        match self {
            SurfaceBsdfType::Emissive => Scattering::Emitted,
            SurfaceBsdfType::Diffuse => diffuse::scatter(in_direction, normal, rng),
            SurfaceBsdfType::Mirror => mirror::scatter(in_direction, normal),
            SurfaceBsdfType::Refractive { properties } => {
                refractive::scatter(properties, in_direction, normal, wavelength, rng)
            }
        }
    }
}

pub(crate) enum Scattering {
    Reflected {
        out_direction: Vector3<f32>,
        probability: f32,
        dispersed: bool,
        brdf: Option<Brdf>,
    },
    Emitted,
}
