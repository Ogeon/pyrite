use super::{
    instruction::{BinaryValueType, Instruction, NumberValue, ValueConversion, VectorValue},
    registers::{NumberRegister, Registers, RgbRegister, VectorRegister},
    ProgramFor, ProgramInput, Resources,
};
use crate::{
    math::{blackbody, fresnel},
    project::expressions::Vector,
};
use cgmath::{Vector4, VectorSpace};
use palette::{LinSrgba, Mix};

pub struct ExecutionContext<'p> {
    registers: Registers,
    resources: Resources<'p>,
}

impl<'p> ExecutionContext<'p> {
    pub(crate) fn new(resources: Resources<'p>) -> Self {
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
            } => {
                self.run_instructions(instructions, input);

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

    fn run_instructions<I: ProgramInput>(
        &mut self,
        instructions: &[Instruction<I::NumberInput, I::VectorInput>],
        input: &I,
    ) {
        self.registers.clear();

        for instruction in instructions {
            match *instruction {
                Instruction::NumberValue { number } => self.registers.push_number(number),
                Instruction::VectorValue { x, y, z, w } => {
                    let x = get_number_value(x, &self.registers, input);
                    let y = get_number_value(y, &self.registers, input);
                    let z = get_number_value(z, &self.registers, input);
                    let w = get_number_value(w, &self.registers, input);
                    self.registers.push_vector(Vector4::new(x, y, z, w).into());
                }
                Instruction::RgbValue { red, green, blue } => {
                    let red = get_number_value(red, &self.registers, input);
                    let green = get_number_value(green, &self.registers, input);
                    let blue = get_number_value(blue, &self.registers, input);
                    self.registers
                        .push_rgb(LinSrgba::new(red, green, blue, 1.0));
                }
                Instruction::SpectrumValue {
                    wavelength,
                    spectrum,
                } => {
                    let wavelength = get_number_value(wavelength, &self.registers, input);
                    let intensity = self.resources.spectra.get(spectrum).get(wavelength);
                    self.registers.push_number(intensity);
                }
                Instruction::ColorTextureValue {
                    texture_coordinates,
                    texture,
                } => {
                    let position = get_vector_value(texture_coordinates, input).into();
                    let color = self
                        .resources
                        .textures
                        .get_color(texture)
                        .get_color(position);
                    self.registers.push_rgb(color);
                }
                Instruction::MonoTextureValue {
                    texture_coordinates,
                    texture,
                } => {
                    let position = get_vector_value(texture_coordinates, input).into();
                    let color = self
                        .resources
                        .textures
                        .get_mono(texture)
                        .get_color(position);
                    self.registers.push_number(color.luma);
                }
                Instruction::RgbSpectrumValue { wavelength, source } => {
                    let wavelength = get_number_value(wavelength, &self.registers, input);

                    let rgb = self.registers.get_rgb(source).color;

                    let red_response = rgb.red * crate::rgb::response::RED.get(wavelength);
                    let green_response = rgb.green * crate::rgb::response::GREEN.get(wavelength);
                    let blue_response = rgb.blue * crate::rgb::response::BLUE.get(wavelength);

                    let intensity = red_response + green_response + blue_response;
                    self.registers.push_number(intensity);
                }
                Instruction::Fresnel {
                    normal,
                    incident,
                    ior,
                    env_ior,
                } => {
                    let ior = get_number_value(ior, &self.registers, input);

                    let env_ior = get_number_value(env_ior, &self.registers, input);

                    let normal = get_vector_value(normal, input);

                    let incident = get_vector_value(incident, input);

                    let value = fresnel(ior, env_ior, normal.into(), incident.into());
                    self.registers.push_number(value);
                }
                Instruction::Blackbody {
                    wavelength,
                    temperature,
                } => {
                    let wavelength = get_number_value(wavelength, &self.registers, input);
                    let temperature = get_number_value(temperature, &self.registers, input);

                    self.registers
                        .push_number(blackbody(wavelength, temperature));
                }
                Instruction::Convert { conversion } => match conversion {
                    ValueConversion::RgbToVector { source } => {
                        let rgb = self.registers.get_rgb(source);

                        let x = (rgb.red * 2.0) - 1.0;
                        let y = (rgb.green * 2.0) - 1.0;
                        let z = (rgb.blue * 2.0) - 1.0;
                        let w = (rgb.alpha * 2.0) - 1.0;

                        self.registers.push_vector(Vector4::new(x, y, z, w).into());
                    }
                },
                Instruction::Mix {
                    value_type,
                    lhs,
                    rhs,
                    amount,
                } => {
                    let amount = get_number_value(amount, &self.registers, input);
                    let amount = amount.min(1.0).max(0.0);

                    match value_type {
                        BinaryValueType::Number => {
                            let lhs = self.registers.get_number(NumberRegister(lhs));
                            let rhs = self.registers.get_number(NumberRegister(rhs));
                            let result = lhs * (1.0 - amount) + rhs * amount;
                            self.registers.push_number(result);
                        }
                        BinaryValueType::Vector => {
                            let lhs: Vector4<f32> =
                                self.registers.get_vector(VectorRegister(lhs)).into();
                            let rhs = self.registers.get_vector(VectorRegister(rhs)).into();
                            let result = lhs.lerp(rhs, amount);
                            self.registers.push_vector(result.into());
                        }
                        BinaryValueType::Rgb => {
                            let lhs = self.registers.get_rgb(RgbRegister(lhs));
                            let rhs = self.registers.get_rgb(RgbRegister(rhs));
                            self.registers.push_rgb(lhs.mix(&rhs, amount));
                        }
                    }
                }
                Instruction::Binary {
                    value_type,
                    operator,
                    lhs,
                    rhs,
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
                        self.registers.push_number(result);
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
                        self.registers.push_vector(result);
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
                        self.registers.push_rgb(result);
                    }
                },
                Instruction::Clamp { value, min, max } => {
                    let value = get_number_value(value, &self.registers, input);
                    let min = get_number_value(min, &self.registers, input);
                    let max = get_number_value(max, &self.registers, input);

                    self.registers.push_number(value.min(max).max(min))
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
