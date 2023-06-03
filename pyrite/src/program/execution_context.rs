use super::{
    instruction::{
        BinaryValueType, Instruction, InstructionType, NumberValue, ValueConversion, VectorValue,
    },
    registers::{NumberRegister, Registers, RgbRegister, VectorRegister},
    Inputs, ProgramFor, ProgramInput, Resources,
};
use crate::{
    math::{blackbody, fresnel},
    project::expressions::Vector,
};
use cgmath::{Vector4, VectorSpace};
use palette::{LinSrgba, Mix};

pub struct ExecutionContext<'p> {
    registers: Registers,
    resources: &'p Resources,
}

impl<'p> ExecutionContext<'p> {
    pub(crate) fn new(resources: &'p Resources) -> Self {
        ExecutionContext {
            registers: Registers::new(),
            resources,
        }
    }

    #[inline]
    pub(crate) fn run<I: ProgramInput, T>(
        &mut self,
        program: ProgramFor<'p, I, T>,
        input: &I,
    ) -> T {
        match program.program_type {
            super::ProgramType::Constant { value, convert } => convert(value),
            super::ProgramType::Instructions {
                instructions,
                output,
                numbers,
                vectors,
                rgb_values,
            } => {
                self.registers.reserve(numbers, vectors, rgb_values);
                self.run_instructions(instructions, input, Inputs::all());

                match output {
                    super::ProgramOutput::FromNumber(register, convert) => {
                        convert(self.registers.get_number(register))
                    }
                    super::ProgramOutput::FromVector(register, convert) => {
                        convert(self.registers.get_vector(register))
                    }
                }
            }
        }
    }

    pub(super) fn memoized<'a, I: ProgramInput, T>(
        &'a mut self,
        program: ProgramFor<'p, I, T>,
    ) -> MemoizedContext<'a, 'p, I, T> {
        MemoizedContext {
            context: self,
            program,
            fresh: true,
        }
    }

    fn run_instructions<I: ProgramInput>(
        &mut self,
        instructions: &[Instruction<I::NumberInput, I::VectorInput>],
        input: &I,
        changes: Inputs,
    ) {
        for instruction in instructions {
            if !instruction.dependencies.is_empty() && !instruction.dependencies.intersects(changes)
            {
                continue;
            }

            match instruction.instruction_type {
                InstructionType::NumberValue { number, output } => {
                    self.registers.set_number(number, output)
                }
                InstructionType::VectorValue { x, y, z, w, output } => {
                    let x = get_number_value(x, &self.registers, input);
                    let y = get_number_value(y, &self.registers, input);
                    let z = get_number_value(z, &self.registers, input);
                    let w = get_number_value(w, &self.registers, input);
                    self.registers
                        .set_vector(Vector4::new(x, y, z, w).into(), output);
                }
                InstructionType::RgbValue {
                    red,
                    green,
                    blue,
                    output,
                } => {
                    let red = get_number_value(red, &self.registers, input);
                    let green = get_number_value(green, &self.registers, input);
                    let blue = get_number_value(blue, &self.registers, input);
                    self.registers
                        .set_rgb(LinSrgba::new(red, green, blue, 1.0), output);
                }
                InstructionType::SpectrumValue {
                    wavelength,
                    spectrum,
                    output,
                } => {
                    let wavelength = get_number_value(wavelength, &self.registers, input);
                    let intensity = self.resources.spectra.get(spectrum).get(wavelength);
                    self.registers.set_number(intensity, output);
                }
                InstructionType::ColorTextureValue {
                    texture_coordinates,
                    texture,
                    output,
                } => {
                    let position = get_vector_value(texture_coordinates, input).into();
                    let color = self
                        .resources
                        .textures
                        .get_color(texture)
                        .get_color(position);
                    self.registers.set_rgb(color, output);
                }
                InstructionType::MonoTextureValue {
                    texture_coordinates,
                    texture,
                    output,
                } => {
                    let position = get_vector_value(texture_coordinates, input).into();
                    let color = self
                        .resources
                        .textures
                        .get_mono(texture)
                        .get_color(position);
                    self.registers.set_number(color.luma, output);
                }
                InstructionType::RgbSpectrumValue {
                    wavelength,
                    source,
                    output,
                } => {
                    let wavelength = get_number_value(wavelength, &self.registers, input);

                    let rgb = self.registers.get_rgb(source).color;
                    let response = rgb * crate::rgb::response::RGB.get(wavelength);

                    let intensity = response.red + response.green + response.blue;
                    self.registers.set_number(intensity, output);
                }
                InstructionType::Fresnel {
                    normal,
                    incident,
                    ior,
                    env_ior,
                    output,
                } => {
                    let ior = get_number_value(ior, &self.registers, input);

                    let env_ior = get_number_value(env_ior, &self.registers, input);

                    let normal = get_vector_value(normal, input);

                    let incident = get_vector_value(incident, input);

                    let value = fresnel(ior, env_ior, normal.into(), incident.into());
                    self.registers.set_number(value, output);
                }
                InstructionType::Blackbody {
                    wavelength,
                    temperature,
                    output,
                } => {
                    let wavelength = get_number_value(wavelength, &self.registers, input);
                    let temperature = get_number_value(temperature, &self.registers, input);

                    self.registers
                        .set_number(blackbody(wavelength, temperature), output);
                }
                InstructionType::Convert { conversion } => match conversion {
                    ValueConversion::RgbToVector { source, output } => {
                        let rgb = self.registers.get_rgb(source);

                        let x = (rgb.red * 2.0) - 1.0;
                        let y = (rgb.green * 2.0) - 1.0;
                        let z = (rgb.blue * 2.0) - 1.0;
                        let w = (rgb.alpha * 2.0) - 1.0;

                        self.registers
                            .set_vector(Vector4::new(x, y, z, w).into(), output);
                    }
                },
                InstructionType::Mix {
                    value_type,
                    lhs,
                    rhs,
                    amount,
                    output,
                } => {
                    let amount = get_number_value(amount, &self.registers, input);
                    let amount = amount.min(1.0).max(0.0);

                    match value_type {
                        BinaryValueType::Number => {
                            let lhs = self.registers.get_number(NumberRegister(lhs));
                            let rhs = self.registers.get_number(NumberRegister(rhs));
                            let result = lhs * (1.0 - amount) + rhs * amount;
                            self.registers.set_number(result, NumberRegister(output));
                        }
                        BinaryValueType::Vector => {
                            let lhs: Vector4<f32> =
                                self.registers.get_vector(VectorRegister(lhs)).into();
                            let rhs = self.registers.get_vector(VectorRegister(rhs)).into();
                            let result = lhs.lerp(rhs, amount);
                            self.registers
                                .set_vector(result.into(), VectorRegister(output));
                        }
                        BinaryValueType::Rgb => {
                            let lhs = self.registers.get_rgb(RgbRegister(lhs));
                            let rhs = self.registers.get_rgb(RgbRegister(rhs));
                            self.registers
                                .set_rgb(lhs.mix(rhs, amount), RgbRegister(output));
                        }
                    }
                }
                InstructionType::Binary {
                    value_type,
                    operator,
                    lhs,
                    rhs,
                    output,
                } => match value_type {
                    BinaryValueType::Number => {
                        let lhs = self.registers.get_number(NumberRegister(lhs));
                        let rhs = self.registers.get_number(NumberRegister(rhs));
                        let result = match operator {
                            crate::project::expressions::BinaryOperator::Add => lhs + rhs,
                            crate::project::expressions::BinaryOperator::Sub => lhs - rhs,
                            crate::project::expressions::BinaryOperator::Mul => lhs * rhs,
                            crate::project::expressions::BinaryOperator::Div => lhs / rhs,
                        };
                        self.registers.set_number(result, NumberRegister(output));
                    }
                    BinaryValueType::Vector => {
                        let lhs = self.registers.get_vector(VectorRegister(lhs));
                        let rhs = self.registers.get_vector(VectorRegister(rhs));
                        let result = match operator {
                            crate::project::expressions::BinaryOperator::Add => lhs + rhs,
                            crate::project::expressions::BinaryOperator::Sub => lhs - rhs,
                            crate::project::expressions::BinaryOperator::Mul => lhs * rhs,
                            crate::project::expressions::BinaryOperator::Div => lhs / rhs,
                        };
                        self.registers.set_vector(result, VectorRegister(output));
                    }
                    BinaryValueType::Rgb => {
                        let lhs = self.registers.get_rgb(RgbRegister(lhs));
                        let rhs = self.registers.get_rgb(RgbRegister(rhs));
                        let result = match operator {
                            crate::project::expressions::BinaryOperator::Add => lhs + rhs,
                            crate::project::expressions::BinaryOperator::Sub => lhs - rhs,
                            crate::project::expressions::BinaryOperator::Mul => lhs * rhs,
                            crate::project::expressions::BinaryOperator::Div => lhs / rhs,
                        };
                        self.registers.set_rgb(result, RgbRegister(output));
                    }
                },
                InstructionType::Clamp {
                    value,
                    min,
                    max,
                    output,
                } => {
                    let value = get_number_value(value, &self.registers, input);
                    let min = get_number_value(min, &self.registers, input);
                    let max = get_number_value(max, &self.registers, input);

                    self.registers.set_number(value.min(max).max(min), output)
                }
            }
        }
    }
}

fn get_number_value<I: ProgramInput>(
    value: NumberValue<I::NumberInput>,
    registers: &Registers,
    input: &I,
) -> f32 {
    match value {
        NumberValue::Constant(value) => value,
        NumberValue::Input(input_value) => input.get_number_input(input_value),
        NumberValue::Register(value) => registers.get_number(value),
    }
}

fn get_vector_value<I: ProgramInput>(value: VectorValue<I::VectorInput>, input: &I) -> Vector {
    match value {
        VectorValue::Input(input_value) => input.get_vector_input(input_value),
    }
}

pub(super) struct MemoizedContext<'a, 'p, I: ProgramInput, T> {
    context: &'a mut ExecutionContext<'p>,
    program: ProgramFor<'p, I, T>,
    fresh: bool,
}

impl<'a, 'p, I: ProgramInput, T> MemoizedContext<'a, 'p, I, T> {
    #[inline]
    pub(crate) fn run(&mut self, input: &I, changes: Inputs) -> T {
        match self.program.program_type {
            super::ProgramType::Constant { value, convert } => convert(value),
            super::ProgramType::Instructions {
                instructions,
                output,
                numbers,
                vectors,
                rgb_values,
            } => {
                let changes = if self.fresh {
                    self.context.registers.reserve(numbers, vectors, rgb_values);
                    self.fresh = false;
                    Inputs::all()
                } else {
                    changes
                };

                self.context.run_instructions(instructions, input, changes);

                match output {
                    super::ProgramOutput::FromNumber(register, convert) => {
                        convert(self.context.registers.get_number(register))
                    }
                    super::ProgramOutput::FromVector(register, convert) => {
                        convert(self.context.registers.get_vector(register))
                    }
                }
            }
        }
    }
}
