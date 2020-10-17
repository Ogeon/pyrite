use std::{borrow::Cow, cell::Cell, convert::TryFrom, error::Error};

use cgmath::{InnerSpace, Point2, Vector3};

use crate::{
    light::{Light, Wavelengths},
    program::{
        ExecutionContext, NumberInput, Program, ProgramCompiler, ProgramFor, ProgramInput,
        VectorInput,
    },
    project::{
        eval_context::{EvalContext, Evaluate, EvaluateOr},
        expressions::{Expression, Expressions, Vector},
        materials::{MaterialId, Materials, SurfaceMaterial as MaterialNode},
    },
    shapes::Normal,
    tracer::{LightProgram, NormalInput, RenderContext},
    utils::Tools,
};

mod diffuse;
mod mirror;
mod refractive;

/// Corresponding to BSDF in the PBR book.
#[derive(Copy, Clone)]
pub(crate) struct Material<'a> {
    surface: SurfaceMaterial<'a>,
    normal_map: Option<ProgramFor<'a, NormalInput, Vector>>,
}

impl<'a> Material<'a> {
    pub(crate) fn from_project(
        material: crate::project::Material,
        programs: ProgramCompiler<'a>,
        expressions: &mut Expressions,
        materials: &Materials,
        allocator: &'a bumpalo::Bump,
    ) -> Result<Self, Box<dyn Error>> {
        Ok(Material {
            surface: SurfaceMaterial::from_project(
                material.surface,
                programs,
                expressions,
                materials,
                allocator,
            )?,
            normal_map: material
                .normal_map
                .map(|program| programs.compile(&program, expressions))
                .transpose()?,
        })
    }

    /// Corresponds to Sample_f in the PBR book.
    pub(crate) fn sample_reflection<'t>(
        &self,
        out_direction: Vector3<f32>,
        texture_coordinate: Point2<f32>,
        normal: Normal,
        wavelengths: &Wavelengths,
        tools: &mut Tools<'t, 'a>,
    ) -> Option<SurfaceInteraction<'t>> {
        let components = self.surface.components;
        let num_components = components.len();
        let component_index = tools.sampler.gen_index(num_components)?;
        let component = components[component_index];

        let mut interaction = component.sample_reflection(
            out_direction,
            texture_coordinate,
            normal,
            wavelengths,
            tools,
        );

        if interaction.pdf == 0.0 {
            interaction.reflectivity.set_all(0.0);
            return Some(interaction);
        }

        let in_direction = interaction.in_direction;
        if num_components > 1 {
            let input = MaterialInput {
                wavelength: wavelengths.hero(),
                wavelength_used: false.into(),
                normal: normal.vector(),
                ray_direction: -out_direction,
                texture_coordinate,
            };

            if interaction.diffuse {
                for (i, component) in components.iter().enumerate() {
                    if i != component_index {
                        interaction.pdf += component.pdf(
                            out_direction,
                            normal,
                            in_direction,
                            &input,
                            tools.execution_context,
                        );
                    }
                }
            } else {
                for (i, component) in components.iter().enumerate() {
                    if i != component_index {
                        interaction.pdf +=
                            component.get_probability(tools.execution_context, &input);
                    }
                }
            }

            interaction.pdf /= num_components as f32;

            if interaction.diffuse {
                let reflected =
                    in_direction.dot(normal.vector()) * out_direction.dot(normal.vector()) > 0.0;
                interaction.reflectivity.set_all(0.0);

                for component in components {
                    if (reflected && component.has_reflection())
                        || (!reflected && component.has_transmission())
                    {
                        interaction.reflectivity += component.evaluate(
                            out_direction,
                            normal,
                            in_direction,
                            texture_coordinate,
                            wavelengths,
                            &input,
                            tools,
                        );
                    }
                }
            }

            if input.wavelength_used.get() {
                interaction.reflectivity.set_single_wavelength();
            }
        }

        Some(interaction)
    }

    // Corresponds to f in the PBR book.
    pub(crate) fn evaluate<'t>(
        &self,
        out_direction: Vector3<f32>,
        normal: Normal,
        in_direction: Vector3<f32>,
        texture_coordinate: Point2<f32>,
        wavelengths: &Wavelengths,
        tools: &mut Tools<'t, 'a>,
    ) -> Light<'t> {
        let mut reflectivity = tools.light_pool.get();
        let reflected =
            in_direction.dot(normal.vector()) * out_direction.dot(normal.vector()) > 0.0;

        let input = MaterialInput {
            wavelength: wavelengths.hero(),
            wavelength_used: false.into(),
            normal: normal.vector(),
            ray_direction: -out_direction,
            texture_coordinate,
        };

        for component in self.surface.components {
            if (reflected && component.has_reflection())
                || (!reflected && component.has_transmission())
            {
                reflectivity += component.evaluate(
                    out_direction,
                    normal,
                    in_direction,
                    texture_coordinate,
                    wavelengths,
                    &input,
                    tools,
                );
            }
        }

        if input.wavelength_used.get() {
            reflectivity.set_single_wavelength();
        }

        reflectivity
    }

    pub(crate) fn pdf(
        &self,
        out_direction: Vector3<f32>,
        normal: Normal,
        in_direction: Vector3<f32>,
        input: &MaterialInput,
        execution_context: &mut ExecutionContext<'a>,
    ) -> f32 {
        if self.surface.components.is_empty() {
            return 0.0; // Should terminate path
        }

        let mut pdf = 0.0;
        for component in self.surface.components {
            pdf += component.pdf(
                out_direction,
                normal,
                in_direction,
                input,
                execution_context,
            );
        }

        pdf / self.surface.components.len() as f32
    }

    pub(crate) fn light_emission<'t>(
        &self,
        out_direction: Vector3<f32>,
        normal: Vector3<f32>,
        texture_coordingate: Point2<f32>,
        wavelengths: &Wavelengths,
        tools: &mut Tools<'t, 'a>,
    ) -> Option<Light<'t>> {
        let input = RenderContext {
            wavelength: wavelengths.hero(),
            normal,
            ray_direction: -out_direction,
            texture: texture_coordingate,
        };

        let mut color_program = self
            .surface
            .emission?
            .memoize(input, tools.execution_context);

        let mut light = tools.light_pool.get();
        for (bin, wavelength) in light.iter_mut().zip(wavelengths) {
            color_program.update_input().set_wavelength(wavelength);
            *bin = color_program.run();
        }

        Some(light)
    }

    pub(crate) fn is_emissive(&self) -> bool {
        self.surface.emission.is_some()
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
    emission: Option<LightProgram<'a>>,
}

impl<'a> SurfaceMaterial<'a> {
    pub(crate) fn from_project(
        material: crate::project::SurfaceMaterial,
        programs: ProgramCompiler<'a>,
        expressions: &mut Expressions,
        materials: &Materials,
        allocator: &'a bumpalo::Bump,
    ) -> Result<Self, Box<dyn Error>> {
        struct StackEntry {
            material: MaterialId,
            probability: Option<Expression>,
        }

        let mut stack = vec![];
        if let Some(reflection) = material.reflection {
            stack.push(StackEntry {
                material: reflection,
                probability: None,
            });
        }

        let mut components = Vec::new();
        let emission = material
            .emission
            .map(|expression| programs.compile(&expression, expressions))
            .transpose()?;

        while let Some(entry) = stack.pop() {
            match materials.get(entry.material) {
                MaterialNode::Diffuse { color } => components.push(MaterialComponent {
                    probability: entry
                        .probability
                        .map(|expression| programs.compile(&expression, expressions))
                        .transpose()?,
                    bsdf: SurfaceBsdf {
                        color: programs.compile(color, expressions)?,
                        bsdf_type: SurfaceBsdfType::Diffuse,
                    },
                }),
                MaterialNode::Mirror { color } => components.push(MaterialComponent {
                    probability: entry
                        .probability
                        .map(|expression| programs.compile(&expression, expressions))
                        .transpose()?,
                    bsdf: SurfaceBsdf {
                        color: programs.compile(color, expressions)?,
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
                    let eval_context = EvalContext { expressions };
                    components.push(MaterialComponent {
                        probability: entry
                            .probability
                            .map(|expression| programs.compile(&expression, expressions))
                            .transpose()?,
                        bsdf: SurfaceBsdf {
                            color: programs.compile(color, expressions)?,
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
                    let amount = expressions.insert_clamp(amount, 0.0.into(), 1.0.into());
                    let lhs_probability = match entry.probability {
                        Some(probability) => expressions.insert_mul(probability, amount),
                        None => amount,
                    };

                    stack.push(StackEntry {
                        material: lhs,
                        probability: Some(lhs_probability),
                    });
                    stack.push(StackEntry {
                        material: rhs,
                        probability: Some(expressions.insert_sub(1.0.into(), lhs_probability)),
                    });
                }
                &MaterialNode::Add { lhs, rhs } => {
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

        Ok(SurfaceMaterial {
            components: allocator.alloc_slice_copy(&components),
            emission,
        })
    }
}

#[derive(Copy, Clone)]
pub(crate) struct MaterialComponent<'a> {
    probability: Option<Program<'a, MaterialNumberInput, MaterialVectorInput, f32>>,
    pub(crate) bsdf: SurfaceBsdf<'a>,
}

impl<'a> MaterialComponent<'a> {
    /// Corresponds to Sample_f in the PBR book.
    fn sample_reflection<'t>(
        &self,
        out_direction: Vector3<f32>,
        texture_coordinate: Point2<f32>,
        normal: Normal,
        wavelengths: &Wavelengths,
        tools: &mut Tools<'t, 'a>,
    ) -> SurfaceInteraction<'t> {
        let mut interaction = self.bsdf.sample_reflection(
            out_direction,
            normal,
            texture_coordinate,
            wavelengths,
            tools,
        );

        let input = MaterialInput {
            wavelength: wavelengths.hero(),
            wavelength_used: false.into(),
            normal: normal.vector(),
            ray_direction: -out_direction,
            texture_coordinate,
        };

        let selection_probability = self.get_probability(tools.execution_context, &input);
        interaction.reflectivity *= selection_probability;
        interaction.pdf *= selection_probability;

        if input.wavelength_used.get() {
            interaction.reflectivity.set_single_wavelength();
        }

        interaction
    }

    fn pdf(
        &self,
        out_direction: Vector3<f32>,
        normal: Normal,
        in_direction: Vector3<f32>,
        input: &MaterialInput,
        execution_context: &mut ExecutionContext<'a>,
    ) -> f32 {
        self.bsdf.pdf(out_direction, normal, in_direction)
            * self.get_probability(execution_context, &input)
    }

    // Corresponds to f in the PBR book.
    fn evaluate<'t>(
        &self,
        out_direction: Vector3<f32>,
        normal: Normal,
        in_direction: Vector3<f32>,
        texture_coordinate: Point2<f32>,
        wavelengths: &Wavelengths,
        input: &MaterialInput,
        tools: &mut Tools<'t, 'a>,
    ) -> Light<'t> {
        self.bsdf.evaluate(
            out_direction,
            normal,
            in_direction,
            texture_coordinate,
            wavelengths,
            tools,
        ) * self.get_probability(tools.execution_context, input)
    }

    fn has_reflection(&self) -> bool {
        self.bsdf.has_reflection()
    }

    fn has_transmission(&self) -> bool {
        self.bsdf.has_transmission()
    }

    fn get_probability(&self, exe: &mut ExecutionContext<'a>, input: &MaterialInput) -> f32 {
        if let Some(program) = self.probability {
            exe.run(program, input) // self.selection_compensation
        } else {
            1.0 // self.selection_compensation
        }
    }
}

pub(crate) struct MaterialInput {
    pub(crate) wavelength: f32,
    pub(crate) wavelength_used: Cell<bool>,
    pub(crate) normal: Vector3<f32>,
    pub(crate) ray_direction: Vector3<f32>,
    pub(crate) texture_coordinate: Point2<f32>,
}

impl ProgramInput for MaterialInput {
    type NumberInput = MaterialNumberInput;
    type VectorInput = MaterialVectorInput;

    fn get_number_input(&self, input: Self::NumberInput) -> f32 {
        match input {
            MaterialNumberInput::Wavelength => {
                self.wavelength_used.set(true);
                self.wavelength
            }
        }
    }

    fn get_vector_input(&self, input: Self::VectorInput) -> Vector {
        match input {
            MaterialVectorInput::Normal => self.normal.into(),
            MaterialVectorInput::RayDirection => self.ray_direction.into(),
            MaterialVectorInput::TextureCoordinates => self.texture_coordinate.into(),
        }
    }
}

#[derive(Copy, Clone)]
pub(crate) enum MaterialNumberInput {
    Wavelength,
}

impl TryFrom<NumberInput> for MaterialNumberInput {
    type Error = Cow<'static, str>;

    fn try_from(value: NumberInput) -> Result<Self, Self::Error> {
        match value {
            NumberInput::Wavelength => Ok(MaterialNumberInput::Wavelength),
        }
    }
}

#[derive(Copy, Clone)]
pub(crate) enum MaterialVectorInput {
    Normal,
    RayDirection,
    TextureCoordinates,
}

impl TryFrom<VectorInput> for MaterialVectorInput {
    type Error = Cow<'static, str>;

    fn try_from(value: VectorInput) -> Result<Self, Self::Error> {
        match value {
            VectorInput::Normal => Ok(MaterialVectorInput::Normal),
            VectorInput::RayDirection => Ok(MaterialVectorInput::RayDirection),
            VectorInput::TextureCoordinates => Ok(MaterialVectorInput::TextureCoordinates),
        }
    }
}

#[derive(Copy, Clone)]
pub(crate) struct SurfaceBsdf<'a> {
    pub color: LightProgram<'a>,
    bsdf_type: SurfaceBsdfType,
}

impl<'a> SurfaceBsdf<'a> {
    /// Corresponds to Sample_f in the PBR book.
    fn sample_reflection<'t>(
        &self,
        out_direction: Vector3<f32>,
        normal: Normal,
        texture_coordinate: Point2<f32>,
        wavelengths: &Wavelengths,
        tools: &mut Tools<'t, 'a>,
    ) -> SurfaceInteraction<'t> {
        self.bsdf_type.sample_reflection(
            out_direction,
            normal,
            texture_coordinate,
            self.color,
            wavelengths,
            tools,
        )
    }

    /// Corresponds to f in the PBR book.
    fn evaluate<'t>(
        &self,
        out_direction: Vector3<f32>,
        normal: Normal,
        in_direction: Vector3<f32>,
        texture_coordinate: Point2<f32>,
        wavelengths: &Wavelengths,
        tools: &mut Tools<'t, 'a>,
    ) -> Light<'t> {
        self.bsdf_type.evaluate(
            out_direction,
            normal,
            in_direction,
            texture_coordinate,
            self.color,
            wavelengths,
            tools,
        )
    }

    fn pdf(&self, out_direction: Vector3<f32>, normal: Normal, in_direction: Vector3<f32>) -> f32 {
        self.bsdf_type.pdf(out_direction, normal, in_direction)
    }

    fn has_reflection(&self) -> bool {
        self.bsdf_type.has_reflection()
    }

    fn has_transmission(&self) -> bool {
        self.bsdf_type.has_transmission()
    }
}

#[derive(Copy, Clone)]
enum SurfaceBsdfType {
    Diffuse,
    Mirror,
    Refractive { properties: refractive::Properties },
}

impl SurfaceBsdfType {
    pub(crate) fn sample_reflection<'t, 'a>(
        &self,
        out_direction: Vector3<f32>,
        normal: Normal,
        texture_coordinate: Point2<f32>,
        color: LightProgram<'a>,
        wavelengths: &Wavelengths,
        tools: &mut Tools<'t, 'a>,
    ) -> SurfaceInteraction<'t> {
        let out_direction = normal.into_space(out_direction);

        let mut interaction = match self {
            SurfaceBsdfType::Diffuse => diffuse::sample_reflection(
                out_direction,
                texture_coordinate,
                color,
                wavelengths,
                tools,
            ),
            SurfaceBsdfType::Mirror => mirror::sample_reflection(
                out_direction,
                texture_coordinate,
                color,
                wavelengths,
                tools,
            ),
            SurfaceBsdfType::Refractive { properties } => refractive::sample_reflection(
                properties,
                out_direction,
                texture_coordinate,
                color,
                wavelengths,
                tools,
            ),
        };

        interaction.in_direction = normal.from_space(interaction.in_direction);
        interaction
    }

    fn evaluate<'t, 'a>(
        &self,
        out_direction: Vector3<f32>,
        normal: Normal,
        in_direction: Vector3<f32>,
        texture_coordinate: Point2<f32>,
        color: LightProgram<'a>,
        wavelengths: &Wavelengths,
        tools: &mut Tools<'t, 'a>,
    ) -> Light<'t> {
        let out_direction = normal.into_space(out_direction);
        let _in_direction = normal.into_space(in_direction);

        match self {
            SurfaceBsdfType::Diffuse => {
                diffuse::evaluate(out_direction, texture_coordinate, color, wavelengths, tools)
            }
            SurfaceBsdfType::Mirror | SurfaceBsdfType::Refractive { .. } => tools.light_pool.get(),
        }
    }

    fn pdf(&self, out_direction: Vector3<f32>, normal: Normal, in_direction: Vector3<f32>) -> f32 {
        let out_direction = normal.into_space(out_direction);
        let in_direction = normal.into_space(in_direction);

        match self {
            SurfaceBsdfType::Diffuse => diffuse::pdf(out_direction, in_direction),
            SurfaceBsdfType::Mirror => 0.0,
            SurfaceBsdfType::Refractive { .. } => 0.0,
        }
    }

    fn has_reflection(&self) -> bool {
        match self {
            SurfaceBsdfType::Diffuse => true,
            SurfaceBsdfType::Mirror => true,
            SurfaceBsdfType::Refractive { .. } => true,
        }
    }

    fn has_transmission(&self) -> bool {
        match self {
            SurfaceBsdfType::Diffuse => false,
            SurfaceBsdfType::Mirror => false,
            SurfaceBsdfType::Refractive { .. } => true,
        }
    }
}

pub(crate) struct SurfaceInteraction<'a> {
    pub reflectivity: Light<'a>,
    pub pdf: f32,
    pub diffuse: bool,
    pub in_direction: Vector3<f32>,
}
