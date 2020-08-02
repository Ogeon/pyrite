use super::{
    instruction::{BinaryValueType, Instruction, NumberValue, ValueConversion},
    registers::{NumberRegister, Registers, RgbRegister, VectorRegister},
    ProgramFor, ProgramInput, Resources,
};
use crate::math::{blackbody, fresnel};
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
                Instruction::NumberInput { input_value } => self
                    .registers
                    .push_number(input.get_number_input(input_value)),
                Instruction::VectorInput { input_value } => self
                    .registers
                    .push_vector(input.get_vector_input(input_value)),
                Instruction::NumberValue { number } => self.registers.push_number(number),
                Instruction::VectorValue { x, y, z, w } => {
                    let x = match x {
                        NumberValue::Constant(x) => x,
                        NumberValue::Register(x) => self.registers.get_number(x),
                    };
                    let y = match y {
                        NumberValue::Constant(y) => y,
                        NumberValue::Register(y) => self.registers.get_number(y),
                    };
                    let z = match z {
                        NumberValue::Constant(z) => z,
                        NumberValue::Register(z) => self.registers.get_number(z),
                    };
                    let w = match w {
                        NumberValue::Constant(w) => w,
                        NumberValue::Register(w) => self.registers.get_number(w),
                    };
                    self.registers.push_vector(Vector4::new(x, y, z, w).into());
                }
                Instruction::RgbValue { red, green, blue } => {
                    let red = match red {
                        NumberValue::Constant(red) => red,
                        NumberValue::Register(red) => self.registers.get_number(red),
                    };
                    let green = match green {
                        NumberValue::Constant(green) => green,
                        NumberValue::Register(green) => self.registers.get_number(green),
                    };
                    let blue = match blue {
                        NumberValue::Constant(blue) => blue,
                        NumberValue::Register(blue) => self.registers.get_number(blue),
                    };
                    self.registers
                        .push_rgb(LinSrgba::new(red, green, blue, 1.0));
                }
                Instruction::SpectrumValue {
                    wavelength,
                    spectrum,
                } => {
                    let wavelength = match wavelength {
                        NumberValue::Constant(wavelength) => wavelength,
                        NumberValue::Register(wavelength) => self.registers.get_number(wavelength),
                    };
                    let intensity = self.resources.spectra.get(spectrum).get(wavelength);
                    self.registers.push_number(intensity);
                }
                Instruction::ColorTextureValue {
                    texture_coordinates,
                    texture,
                } => {
                    let position = self.registers.get_vector(texture_coordinates).into();
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
                    let position = self.registers.get_vector(texture_coordinates).into();
                    let color = self
                        .resources
                        .textures
                        .get_mono(texture)
                        .get_color(position);
                    self.registers.push_number(color.luma);
                }
                Instruction::RgbSpectrumValue { wavelength, source } => {
                    let wavelength = match wavelength {
                        NumberValue::Constant(wavelength) => wavelength,
                        NumberValue::Register(wavelength) => self.registers.get_number(wavelength),
                    };

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
                    let ior = match ior {
                        NumberValue::Constant(ior) => ior,
                        NumberValue::Register(ior) => self.registers.get_number(ior),
                    };

                    let env_ior = match env_ior {
                        NumberValue::Constant(env_ior) => env_ior,
                        NumberValue::Register(env_ior) => self.registers.get_number(env_ior),
                    };

                    let normal = self.registers.get_vector(normal);

                    let incident = self.registers.get_vector(incident);

                    let value = fresnel(ior, env_ior, normal.into(), incident.into());
                    self.registers.push_number(value);
                }
                Instruction::Blackbody {
                    wavelength,
                    temperature,
                } => {
                    let wavelength = match wavelength {
                        NumberValue::Constant(wavelength) => wavelength,
                        NumberValue::Register(wavelength) => self.registers.get_number(wavelength),
                    };

                    let temperature = match temperature {
                        NumberValue::Constant(temperature) => temperature,
                        NumberValue::Register(temperature) => {
                            self.registers.get_number(temperature)
                        }
                    };

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
                    let amount = match amount {
                        NumberValue::Constant(amount) => amount,
                        NumberValue::Register(amount) => self.registers.get_number(amount),
                    };
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
                    let value = match value {
                        NumberValue::Constant(value) => value,
                        NumberValue::Register(value) => self.registers.get_number(value),
                    };
                    let min = match min {
                        NumberValue::Constant(min) => min,
                        NumberValue::Register(min) => self.registers.get_number(min),
                    };
                    let max = match max {
                        NumberValue::Constant(max) => max,
                        NumberValue::Register(max) => self.registers.get_number(max),
                    };

                    self.registers.push_number(value.min(max).max(min))
                }
            }
        }
    }
}
