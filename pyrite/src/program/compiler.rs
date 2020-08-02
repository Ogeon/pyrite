use std::{
    borrow::Cow,
    collections::HashMap,
    convert::{TryFrom, TryInto},
    error::Error,
};

use bumpalo::Bump;

use crate::project::expressions::{ComplexExpression, Expression, ExpressionId, Expressions};

use super::{
    instruction::{BinaryValueType, Instruction, NumberValue, ValueConversion, VectorValue},
    registers::{NumberRegister, RgbRegister, VectorRegister},
    FromValue, NumberInput, Program, ProgramOutput, ProgramOutputType, ProgramType, VectorInput,
};

macro_rules! unwrap_or_push {
    ($e: expr, $expression_id: expr, $pending: expr) => {
        match $e {
            Ok(value) => value,
            Err(child_id) => {
                $pending.push($expression_id);
                $pending.push(child_id);
                continue;
            }
        }
    };
}

#[derive(Copy, Clone)]
pub(crate) struct ProgramCompiler<'p> {
    arena: &'p Bump,
}

impl<'p> ProgramCompiler<'p> {
    pub(crate) fn new(arena: &'p Bump) -> Self {
        ProgramCompiler { arena }
    }

    pub(crate) fn compile<N, V, T>(
        &self,
        expression: &Expression,
        expressions: &Expressions,
    ) -> Result<Program<'p, N, V, T>, Box<dyn Error>>
    where
        N: Copy + TryFrom<NumberInput, Error = Cow<'static, str>>,
        V: Copy + TryFrom<VectorInput, Error = Cow<'static, str>>,
        T: FromValue,
    {
        let mut status = HashMap::new();
        let mut pending = Vec::new();

        let result_id = match *expression {
            Expression::Number(number) => {
                return Ok(Program {
                    program_type: ProgramType::Constant {
                        value: number as f32,
                        convert: T::from_number()?,
                    },
                })
            }
            Expression::Complex(expression_id) => {
                status.insert(
                    expression_id,
                    ExpressionStatus::Pending(expressions.get(expression_id)),
                );
                pending.push(expression_id);
                expression_id
            }
        };

        let mut instructions = Vec::new();
        let mut next_number_register = 0;
        let mut next_vector_register = 0;
        let mut next_rgb_register = 0;

        while let Some(expression_id) = pending.pop() {
            match status[&expression_id] {
                ExpressionStatus::Pending(&ComplexExpression::Vector { x, y, z, w }) => {
                    let x = try_get_number_value(
                        x,
                        &mut status,
                        expressions,
                        &mut instructions,
                        &mut next_number_register,
                    )?;
                    let x = unwrap_or_push!(x, expression_id, pending);

                    let y = try_get_number_value(
                        y,
                        &mut status,
                        expressions,
                        &mut instructions,
                        &mut next_number_register,
                    )?;
                    let y = unwrap_or_push!(y, expression_id, pending);

                    let z = try_get_number_value(
                        z,
                        &mut status,
                        expressions,
                        &mut instructions,
                        &mut next_number_register,
                    )?;
                    let z = unwrap_or_push!(z, expression_id, pending);

                    let w = try_get_number_value(
                        w,
                        &mut status,
                        expressions,
                        &mut instructions,
                        &mut next_number_register,
                    )?;
                    let w = unwrap_or_push!(w, expression_id, pending);

                    instructions.push(Instruction::VectorValue { x, y, z, w });
                    status.insert(
                        expression_id,
                        ExpressionStatus::Done(Register::Vector(VectorRegister(
                            next_vector_register,
                        ))),
                    );
                    next_vector_register += 1;
                }
                ExpressionStatus::Pending(&ComplexExpression::Rgb { red, green, blue }) => {
                    let red = try_get_number_value(
                        red,
                        &mut status,
                        expressions,
                        &mut instructions,
                        &mut next_number_register,
                    )?;
                    let red = unwrap_or_push!(red, expression_id, pending);

                    let green = try_get_number_value(
                        green,
                        &mut status,
                        expressions,
                        &mut instructions,
                        &mut next_number_register,
                    )?;
                    let green = unwrap_or_push!(green, expression_id, pending);

                    let blue = try_get_number_value(
                        blue,
                        &mut status,
                        expressions,
                        &mut instructions,
                        &mut next_number_register,
                    )?;
                    let blue = unwrap_or_push!(blue, expression_id, pending);

                    instructions.push(Instruction::RgbValue { red, green, blue });
                    status.insert(
                        expression_id,
                        ExpressionStatus::Done(Register::Rgb(RgbRegister(next_rgb_register))),
                    );
                    next_rgb_register += 1;
                }
                ExpressionStatus::Pending(&ComplexExpression::Fresnel { ior, env_ior }) => {
                    let normal = get_vector_input(VectorInput::Normal)?;

                    let incident = get_vector_input(VectorInput::Incident)?;

                    let ior = try_get_number_value(
                        ior,
                        &mut status,
                        expressions,
                        &mut instructions,
                        &mut next_number_register,
                    )?;
                    let ior = unwrap_or_push!(ior, expression_id, pending);

                    let env_ior = try_get_number_value(
                        env_ior,
                        &mut status,
                        expressions,
                        &mut instructions,
                        &mut next_number_register,
                    )?;
                    let env_ior = unwrap_or_push!(env_ior, expression_id, pending);

                    instructions.push(Instruction::Fresnel {
                        ior,
                        env_ior,
                        normal,
                        incident,
                    });
                    status.insert(
                        expression_id,
                        ExpressionStatus::Done(Register::Number(NumberRegister(
                            next_number_register,
                        ))),
                    );
                    next_number_register += 1;
                }
                ExpressionStatus::Pending(&ComplexExpression::Blackbody { temperature }) => {
                    let wavelength = get_number_input(NumberInput::Wavelength)?;

                    let temperature = try_get_number_value(
                        temperature,
                        &mut status,
                        expressions,
                        &mut instructions,
                        &mut next_number_register,
                    )?;
                    let temperature = unwrap_or_push!(temperature, expression_id, pending);

                    instructions.push(Instruction::Blackbody {
                        wavelength,
                        temperature,
                    });
                    status.insert(
                        expression_id,
                        ExpressionStatus::Done(Register::Number(NumberRegister(
                            next_number_register,
                        ))),
                    );
                    next_number_register += 1;
                }
                ExpressionStatus::Pending(&ComplexExpression::Spectrum { points }) => {
                    let wavelength = get_number_input(NumberInput::Wavelength)?;

                    instructions.push(Instruction::SpectrumValue {
                        wavelength,
                        spectrum: points,
                    });

                    status.insert(
                        expression_id,
                        ExpressionStatus::Done(Register::Number(NumberRegister(
                            next_number_register,
                        ))),
                    );
                    next_number_register += 1;
                }
                ExpressionStatus::Pending(&ComplexExpression::ColorTexture { texture }) => {
                    let texture_coordinates = get_vector_input(VectorInput::TextureCoordinates)?;

                    instructions.push(Instruction::ColorTextureValue {
                        texture_coordinates,
                        texture,
                    });
                    status.insert(
                        expression_id,
                        ExpressionStatus::Done(Register::Rgb(RgbRegister(next_rgb_register))),
                    );
                    next_rgb_register += 1;
                }
                ExpressionStatus::Pending(&ComplexExpression::MonoTexture { texture }) => {
                    let texture_coordinates = get_vector_input(VectorInput::TextureCoordinates)?;

                    instructions.push(Instruction::MonoTextureValue {
                        texture_coordinates,
                        texture,
                    });
                    status.insert(
                        expression_id,
                        ExpressionStatus::Done(Register::Number(NumberRegister(
                            next_number_register,
                        ))),
                    );
                    next_number_register += 1;
                }
                ExpressionStatus::Pending(&ComplexExpression::Mix { amount, lhs, rhs }) => {
                    let amount = try_get_number_value(
                        amount,
                        &mut status,
                        expressions,
                        &mut instructions,
                        &mut next_number_register,
                    )?;
                    let amount = unwrap_or_push!(amount, expression_id, pending);

                    let lhs = unwrap_or_push!(
                        try_get_register(lhs, &mut status, expressions),
                        expression_id,
                        pending
                    );

                    let rhs = unwrap_or_push!(
                        try_get_register(rhs, &mut status, expressions),
                        expression_id,
                        pending
                    );

                    let (value_type, lhs, rhs) = convert_operands(
                        lhs,
                        rhs,
                        &mut instructions,
                        &mut next_number_register,
                        &mut next_vector_register,
                        &mut next_rgb_register,
                    );

                    instructions.push(Instruction::Mix {
                        value_type,
                        lhs,
                        rhs,
                        amount,
                    });
                    match value_type {
                        BinaryValueType::Number => {
                            status.insert(
                                expression_id,
                                ExpressionStatus::Done(Register::Number(NumberRegister(
                                    next_number_register,
                                ))),
                            );
                            next_number_register += 1;
                        }
                        BinaryValueType::Vector => {
                            status.insert(
                                expression_id,
                                ExpressionStatus::Done(Register::Vector(VectorRegister(
                                    next_vector_register,
                                ))),
                            );
                            next_vector_register += 1;
                        }
                        BinaryValueType::Rgb => {
                            status.insert(
                                expression_id,
                                ExpressionStatus::Done(Register::Rgb(RgbRegister(
                                    next_rgb_register,
                                ))),
                            );
                            next_rgb_register += 1;
                        }
                    }
                }
                ExpressionStatus::Pending(&ComplexExpression::Binary { operator, lhs, rhs }) => {
                    let lhs = unwrap_or_push!(
                        try_get_register(lhs, &mut status, expressions),
                        expression_id,
                        pending
                    );

                    let rhs = unwrap_or_push!(
                        try_get_register(rhs, &mut status, expressions),
                        expression_id,
                        pending
                    );

                    let (value_type, lhs, rhs) = convert_operands(
                        lhs,
                        rhs,
                        &mut instructions,
                        &mut next_number_register,
                        &mut next_vector_register,
                        &mut next_rgb_register,
                    );

                    instructions.push(Instruction::Binary {
                        value_type,
                        operator,
                        lhs,
                        rhs,
                    });
                    match value_type {
                        BinaryValueType::Number => {
                            status.insert(
                                expression_id,
                                ExpressionStatus::Done(Register::Number(NumberRegister(
                                    next_number_register,
                                ))),
                            );
                            next_number_register += 1;
                        }
                        BinaryValueType::Vector => {
                            status.insert(
                                expression_id,
                                ExpressionStatus::Done(Register::Vector(VectorRegister(
                                    next_vector_register,
                                ))),
                            );
                            next_vector_register += 1;
                        }
                        BinaryValueType::Rgb => {
                            status.insert(
                                expression_id,
                                ExpressionStatus::Done(Register::Rgb(RgbRegister(
                                    next_rgb_register,
                                ))),
                            );
                            next_rgb_register += 1;
                        }
                    }
                }
                ExpressionStatus::Pending(&ComplexExpression::Clamp { value, min, max }) => {
                    let value = try_get_number_value(
                        value,
                        &mut status,
                        expressions,
                        &mut instructions,
                        &mut next_number_register,
                    )?;
                    let value = unwrap_or_push!(value, expression_id, pending);

                    let min = try_get_number_value(
                        min,
                        &mut status,
                        expressions,
                        &mut instructions,
                        &mut next_number_register,
                    )?;
                    let min = unwrap_or_push!(min, expression_id, pending);

                    let max = try_get_number_value(
                        max,
                        &mut status,
                        expressions,
                        &mut instructions,
                        &mut next_number_register,
                    )?;
                    let max = unwrap_or_push!(max, expression_id, pending);

                    instructions.push(Instruction::Clamp { value, min, max });
                    status.insert(
                        expression_id,
                        ExpressionStatus::Done(Register::Number(NumberRegister(
                            next_number_register,
                        ))),
                    );
                    next_number_register += 1;
                }
                ExpressionStatus::Done(_) => {}
            }
        }

        if let Some(ExpressionStatus::Done(register)) = status.remove(&result_id) {
            let output = match (register, T::from_value()) {
                (Register::Number(register), ProgramOutputType::Number(convert)) => {
                    ProgramOutput::FromNumber(register, convert)
                }

                (Register::Number(register), ProgramOutputType::Vector(convert)) => {
                    instructions.push(Instruction::VectorValue {
                        x: NumberValue::Register(register),
                        y: NumberValue::Register(register),
                        z: NumberValue::Register(register),
                        w: NumberValue::Register(register),
                    });
                    let register = VectorRegister(next_vector_register);
                    ProgramOutput::FromVector(register, convert)
                }
                (Register::Vector(register), ProgramOutputType::Vector(convert)) => {
                    ProgramOutput::FromVector(register, convert)
                }

                (Register::Rgb(register), ProgramOutputType::Number(convert)) => {
                    let wavelength = get_number_input(NumberInput::Wavelength)?;

                    instructions.push(Instruction::RgbSpectrumValue {
                        wavelength,
                        source: register,
                    });
                    let register = NumberRegister(next_number_register);
                    ProgramOutput::FromNumber(register, convert)
                }
                (Register::Rgb(register), ProgramOutputType::Vector(convert)) => {
                    instructions.push(Instruction::Convert {
                        conversion: ValueConversion::RgbToVector { source: register },
                    });
                    let register = VectorRegister(next_vector_register);
                    ProgramOutput::FromVector(register, convert)
                }

                (Register::Vector(_), ProgramOutputType::Number(_)) => {
                    return Err("cannot use a vector as a number".into())
                }
            };

            Ok(Program {
                program_type: ProgramType::Instructions {
                    instructions: self.arena.alloc_slice_copy(&instructions),
                    output,
                },
            })
        } else {
            Err("the expression was not compiled to completion".into())
        }
    }
}

enum ExpressionStatus<'a> {
    Pending(&'a ComplexExpression),
    Done(Register),
}

#[derive(Copy, Clone)]
enum Register {
    Number(NumberRegister),
    Vector(VectorRegister),
    Rgb(RgbRegister),
}

enum NumberOrRegister {
    Number(f32),
    Register(Register),
}

fn try_get_register<'a>(
    expression: Expression,
    status: &mut HashMap<ExpressionId, ExpressionStatus<'a>>,
    expressions: &'a Expressions,
) -> Result<NumberOrRegister, ExpressionId> {
    match expression {
        Expression::Number(number) => Ok(NumberOrRegister::Number(number as f32)),
        Expression::Complex(expression_id) => {
            let status = status
                .entry(expression_id)
                .or_insert_with(|| ExpressionStatus::Pending(expressions.get(expression_id)));

            match *status {
                ExpressionStatus::Done(register) => Ok(NumberOrRegister::Register(register)),
                ExpressionStatus::Pending(_) => Err(expression_id),
            }
        }
    }
}

fn try_get_number_value<'a, N, V>(
    expression: Expression,
    status: &mut HashMap<ExpressionId, ExpressionStatus<'a>>,
    expressions: &'a Expressions,
    instructions: &mut Vec<Instruction<N, V>>,
    next_number_register: &mut usize,
) -> Result<Result<NumberValue<N>, ExpressionId>, Box<dyn Error>>
where
    N: TryFrom<NumberInput, Error = Cow<'static, str>>,
{
    match try_get_register(expression, status, expressions) {
        Ok(NumberOrRegister::Number(number)) => Ok(Ok(NumberValue::Constant(number))),
        Ok(NumberOrRegister::Register(Register::Number(register))) => {
            Ok(Ok(NumberValue::Register(register)))
        }
        Ok(NumberOrRegister::Register(Register::Vector(_))) => {
            Err("cannot use a vector as a number".into())
        }
        Ok(NumberOrRegister::Register(Register::Rgb(rgb_register))) => {
            let wavelength = get_number_input(NumberInput::Wavelength)?;

            instructions.push(Instruction::RgbSpectrumValue {
                wavelength,
                source: rgb_register,
            });
            let register = NumberRegister(*next_number_register);
            *next_number_register += 1;
            Ok(Ok(NumberValue::Register(register)))
        }
        Err(child_id) => Ok(Err(child_id)),
    }
}

fn convert_operands<N, V>(
    lhs: NumberOrRegister,
    rhs: NumberOrRegister,
    instructions: &mut Vec<Instruction<N, V>>,
    next_number_register: &mut usize,
    next_vector_register: &mut usize,
    next_rgb_register: &mut usize,
) -> (BinaryValueType, usize, usize) {
    match (lhs, rhs) {
        (NumberOrRegister::Number(lhs), NumberOrRegister::Number(rhs)) => {
            instructions.push(Instruction::NumberValue { number: lhs });
            instructions.push(Instruction::NumberValue { number: rhs });
            let lhs = *next_number_register;
            let rhs = *next_number_register + 1;
            *next_number_register += 2;

            (BinaryValueType::Number, lhs, rhs)
        }
        (NumberOrRegister::Number(lhs), NumberOrRegister::Register(Register::Number(rhs))) => {
            instructions.push(Instruction::NumberValue { number: lhs });
            let lhs = *next_number_register;
            *next_number_register += 1;

            (BinaryValueType::Number, lhs, rhs.0)
        }
        (NumberOrRegister::Number(lhs), NumberOrRegister::Register(Register::Rgb(rhs))) => {
            instructions.push(Instruction::RgbValue {
                red: NumberValue::Constant(lhs),
                green: NumberValue::Constant(lhs),
                blue: NumberValue::Constant(lhs),
            });
            let lhs = *next_rgb_register;
            *next_rgb_register += 1;

            (BinaryValueType::Rgb, lhs, rhs.0)
        }
        (NumberOrRegister::Number(lhs), NumberOrRegister::Register(Register::Vector(rhs))) => {
            instructions.push(Instruction::VectorValue {
                x: NumberValue::Constant(lhs),
                y: NumberValue::Constant(lhs),
                z: NumberValue::Constant(lhs),
                w: NumberValue::Constant(lhs),
            });
            let lhs = *next_vector_register;
            *next_vector_register += 1;

            (BinaryValueType::Vector, lhs, rhs.0)
        }
        (NumberOrRegister::Register(Register::Number(lhs)), NumberOrRegister::Number(rhs)) => {
            instructions.push(Instruction::NumberValue { number: rhs });
            let rhs = *next_number_register;
            *next_number_register += 1;

            (BinaryValueType::Number, lhs.0, rhs)
        }
        (NumberOrRegister::Register(Register::Rgb(lhs)), NumberOrRegister::Number(rhs)) => {
            instructions.push(Instruction::RgbValue {
                red: NumberValue::Constant(rhs),
                green: NumberValue::Constant(rhs),
                blue: NumberValue::Constant(rhs),
            });
            let rhs = *next_rgb_register;
            *next_rgb_register += 1;

            (BinaryValueType::Rgb, lhs.0, rhs)
        }
        (NumberOrRegister::Register(Register::Vector(lhs)), NumberOrRegister::Number(rhs)) => {
            instructions.push(Instruction::VectorValue {
                x: NumberValue::Constant(rhs),
                y: NumberValue::Constant(rhs),
                z: NumberValue::Constant(rhs),
                w: NumberValue::Constant(rhs),
            });
            let rhs = *next_vector_register;
            *next_vector_register += 1;

            (BinaryValueType::Vector, lhs.0, rhs)
        }
        (
            NumberOrRegister::Register(Register::Number(lhs)),
            NumberOrRegister::Register(Register::Number(rhs)),
        ) => (BinaryValueType::Number, lhs.0, rhs.0),
        (
            NumberOrRegister::Register(Register::Number(lhs)),
            NumberOrRegister::Register(Register::Vector(rhs)),
        ) => {
            instructions.push(Instruction::VectorValue {
                x: NumberValue::Register(lhs),
                y: NumberValue::Register(lhs),
                z: NumberValue::Register(lhs),
                w: NumberValue::Register(lhs),
            });
            let lhs = *next_vector_register;
            *next_vector_register += 1;

            (BinaryValueType::Vector, lhs, rhs.0)
        }
        (
            NumberOrRegister::Register(Register::Number(lhs)),
            NumberOrRegister::Register(Register::Rgb(rhs)),
        ) => {
            instructions.push(Instruction::RgbValue {
                red: NumberValue::Register(lhs),
                green: NumberValue::Register(lhs),
                blue: NumberValue::Register(lhs),
            });
            let lhs = *next_rgb_register;
            *next_rgb_register += 1;

            (BinaryValueType::Rgb, lhs, rhs.0)
        }
        (
            NumberOrRegister::Register(Register::Vector(lhs)),
            NumberOrRegister::Register(Register::Number(rhs)),
        ) => {
            instructions.push(Instruction::VectorValue {
                x: NumberValue::Register(rhs),
                y: NumberValue::Register(rhs),
                z: NumberValue::Register(rhs),
                w: NumberValue::Register(rhs),
            });
            let rhs = *next_vector_register;
            *next_vector_register += 1;

            (BinaryValueType::Vector, lhs.0, rhs)
        }
        (
            NumberOrRegister::Register(Register::Vector(lhs)),
            NumberOrRegister::Register(Register::Vector(rhs)),
        ) => (BinaryValueType::Vector, lhs.0, rhs.0),
        (
            NumberOrRegister::Register(Register::Vector(lhs)),
            NumberOrRegister::Register(Register::Rgb(rhs)),
        ) => {
            instructions.push(Instruction::Convert {
                conversion: ValueConversion::RgbToVector { source: rhs },
            });
            let rhs = *next_vector_register;
            *next_vector_register += 1;

            (BinaryValueType::Vector, lhs.0, rhs)
        }
        (
            NumberOrRegister::Register(Register::Rgb(lhs)),
            NumberOrRegister::Register(Register::Number(rhs)),
        ) => {
            instructions.push(Instruction::RgbValue {
                red: NumberValue::Register(rhs),
                green: NumberValue::Register(rhs),
                blue: NumberValue::Register(rhs),
            });
            let rhs = *next_rgb_register;
            *next_rgb_register += 1;

            (BinaryValueType::Rgb, lhs.0, rhs)
        }
        (
            NumberOrRegister::Register(Register::Rgb(lhs)),
            NumberOrRegister::Register(Register::Vector(rhs)),
        ) => {
            instructions.push(Instruction::Convert {
                conversion: ValueConversion::RgbToVector { source: lhs },
            });
            let lhs = *next_vector_register;
            *next_vector_register += 1;

            (BinaryValueType::Vector, lhs, rhs.0)
        }
        (
            NumberOrRegister::Register(Register::Rgb(lhs)),
            NumberOrRegister::Register(Register::Rgb(rhs)),
        ) => (BinaryValueType::Rgb, lhs.0, rhs.0),
    }
}

fn get_number_input<N>(input: NumberInput) -> Result<NumberValue<N>, Box<dyn Error>>
where
    N: TryFrom<NumberInput, Error = Cow<'static, str>>,
{
    Ok(NumberValue::Input(input.try_into()?))
}

fn get_vector_input<V>(input: VectorInput) -> Result<VectorValue<V>, Box<dyn Error>>
where
    V: TryFrom<VectorInput, Error = Cow<'static, str>>,
{
    Ok(VectorValue::Input(input.try_into()?))
}
