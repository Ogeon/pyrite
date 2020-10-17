use std::{
    borrow::Cow,
    collections::HashMap,
    convert::{TryFrom, TryInto},
    error::Error,
};

use bumpalo::Bump;

use crate::project::expressions::{ComplexExpression, Expression, ExpressionId, Expressions};

use super::{
    instruction::{
        BinaryValueType, Instruction, InstructionType, NumberValue, ValueConversion, VectorValue,
    },
    registers::{NumberRegister, RgbRegister, VectorRegister},
    FromValue, Inputs, NumberInput, Program, ProgramOutput, ProgramOutputType, ProgramType,
    VectorInput,
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
        let mut number_registers = RegisterCounter::<NumberRegister>::new();
        let mut vector_registers = RegisterCounter::<VectorRegister>::new();
        let mut rgb_registers = RegisterCounter::<RgbRegister>::new();

        while let Some(expression_id) = pending.pop() {
            match status[&expression_id] {
                ExpressionStatus::Pending(&ComplexExpression::Vector { x, y, z, w }) => {
                    let x = try_get_number_value(
                        x,
                        &mut status,
                        expressions,
                        &mut instructions,
                        &mut number_registers,
                    )?;
                    let (x, x_deps) = unwrap_or_push!(x, expression_id, pending);

                    let y = try_get_number_value(
                        y,
                        &mut status,
                        expressions,
                        &mut instructions,
                        &mut number_registers,
                    )?;
                    let (y, y_deps) = unwrap_or_push!(y, expression_id, pending);

                    let z = try_get_number_value(
                        z,
                        &mut status,
                        expressions,
                        &mut instructions,
                        &mut number_registers,
                    )?;
                    let (z, z_deps) = unwrap_or_push!(z, expression_id, pending);

                    let w = try_get_number_value(
                        w,
                        &mut status,
                        expressions,
                        &mut instructions,
                        &mut number_registers,
                    )?;
                    let (w, w_deps) = unwrap_or_push!(w, expression_id, pending);

                    let output = vector_registers.next();
                    let dependencies = x_deps | y_deps | z_deps | w_deps;
                    instructions.push(Instruction {
                        instruction_type: InstructionType::VectorValue { x, y, z, w, output },
                        dependencies,
                    });
                    status.insert(
                        expression_id,
                        ExpressionStatus::Done {
                            register: Register::Vector(output),
                            dependencies,
                        },
                    );
                }
                ExpressionStatus::Pending(&ComplexExpression::Rgb { red, green, blue }) => {
                    let red = try_get_number_value(
                        red,
                        &mut status,
                        expressions,
                        &mut instructions,
                        &mut number_registers,
                    )?;
                    let (red, red_deps) = unwrap_or_push!(red, expression_id, pending);

                    let green = try_get_number_value(
                        green,
                        &mut status,
                        expressions,
                        &mut instructions,
                        &mut number_registers,
                    )?;
                    let (green, green_deps) = unwrap_or_push!(green, expression_id, pending);

                    let blue = try_get_number_value(
                        blue,
                        &mut status,
                        expressions,
                        &mut instructions,
                        &mut number_registers,
                    )?;
                    let (blue, blue_deps) = unwrap_or_push!(blue, expression_id, pending);

                    let output = rgb_registers.next();
                    let dependencies = red_deps | green_deps | blue_deps;
                    instructions.push(Instruction {
                        instruction_type: InstructionType::RgbValue {
                            red,
                            green,
                            blue,
                            output,
                        },
                        dependencies,
                    });
                    status.insert(
                        expression_id,
                        ExpressionStatus::Done {
                            register: Register::Rgb(output),
                            dependencies,
                        },
                    );
                }
                ExpressionStatus::Pending(&ComplexExpression::Fresnel { ior, env_ior }) => {
                    let (normal, normal_deps) = get_vector_input(VectorInput::Normal)?;

                    let (ray_direction, ray_direction_deps) =
                        get_vector_input(VectorInput::RayDirection)?;

                    let ior = try_get_number_value(
                        ior,
                        &mut status,
                        expressions,
                        &mut instructions,
                        &mut number_registers,
                    )?;
                    let (ior, ior_deps) = unwrap_or_push!(ior, expression_id, pending);

                    let env_ior = try_get_number_value(
                        env_ior,
                        &mut status,
                        expressions,
                        &mut instructions,
                        &mut number_registers,
                    )?;
                    let (env_ior, env_ior_deps) = unwrap_or_push!(env_ior, expression_id, pending);

                    let output = number_registers.next();
                    let dependencies = normal_deps | ray_direction_deps | ior_deps | env_ior_deps;
                    instructions.push(Instruction {
                        instruction_type: InstructionType::Fresnel {
                            ior,
                            env_ior,
                            normal,
                            incident: ray_direction,
                            output,
                        },
                        dependencies,
                    });
                    status.insert(
                        expression_id,
                        ExpressionStatus::Done {
                            register: Register::Number(output),
                            dependencies,
                        },
                    );
                }
                ExpressionStatus::Pending(&ComplexExpression::Blackbody { temperature }) => {
                    let (wavelength, wavelength_deps) = get_number_input(NumberInput::Wavelength)?;

                    let temperature = try_get_number_value(
                        temperature,
                        &mut status,
                        expressions,
                        &mut instructions,
                        &mut number_registers,
                    )?;
                    let (temperature, temperature_deps) =
                        unwrap_or_push!(temperature, expression_id, pending);

                    let output = number_registers.next();
                    let dependencies = wavelength_deps | temperature_deps;
                    instructions.push(Instruction {
                        instruction_type: InstructionType::Blackbody {
                            wavelength,
                            temperature,
                            output,
                        },
                        dependencies,
                    });
                    status.insert(
                        expression_id,
                        ExpressionStatus::Done {
                            register: Register::Number(output),
                            dependencies,
                        },
                    );
                }
                ExpressionStatus::Pending(&ComplexExpression::Spectrum { points }) => {
                    let (wavelength, dependencies) = get_number_input(NumberInput::Wavelength)?;

                    let output = number_registers.next();
                    instructions.push(Instruction {
                        instruction_type: InstructionType::SpectrumValue {
                            wavelength,
                            spectrum: points,
                            output,
                        },
                        dependencies,
                    });

                    status.insert(
                        expression_id,
                        ExpressionStatus::Done {
                            register: Register::Number(output),
                            dependencies,
                        },
                    );
                }
                ExpressionStatus::Pending(&ComplexExpression::ColorTexture { texture }) => {
                    let (texture_coordinates, dependencies) =
                        get_vector_input(VectorInput::TextureCoordinates)?;

                    let output = rgb_registers.next();
                    instructions.push(Instruction {
                        instruction_type: InstructionType::ColorTextureValue {
                            texture_coordinates,
                            texture,
                            output,
                        },
                        dependencies,
                    });
                    status.insert(
                        expression_id,
                        ExpressionStatus::Done {
                            register: Register::Rgb(output),
                            dependencies,
                        },
                    );
                }
                ExpressionStatus::Pending(&ComplexExpression::MonoTexture { texture }) => {
                    let (texture_coordinates, dependencies) =
                        get_vector_input(VectorInput::TextureCoordinates)?;

                    let output = number_registers.next();
                    instructions.push(Instruction {
                        instruction_type: InstructionType::MonoTextureValue {
                            texture_coordinates,
                            texture,
                            output,
                        },
                        dependencies,
                    });
                    status.insert(
                        expression_id,
                        ExpressionStatus::Done {
                            register: Register::Number(output),
                            dependencies,
                        },
                    );
                }
                ExpressionStatus::Pending(&ComplexExpression::Mix { amount, lhs, rhs }) => {
                    let amount = try_get_number_value(
                        amount,
                        &mut status,
                        expressions,
                        &mut instructions,
                        &mut number_registers,
                    )?;
                    let (amount, amount_deps) = unwrap_or_push!(amount, expression_id, pending);

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

                    let (value_type, (lhs, lhs_deps), (rhs, rhs_deps)) = convert_operands(
                        lhs,
                        rhs,
                        &mut instructions,
                        &mut number_registers,
                        &mut vector_registers,
                        &mut rgb_registers,
                    );

                    let dependencies = amount_deps | lhs_deps | rhs_deps;
                    let output = match value_type {
                        BinaryValueType::Number => {
                            let output = number_registers.next();
                            status.insert(
                                expression_id,
                                ExpressionStatus::Done {
                                    register: Register::Number(output),
                                    dependencies,
                                },
                            );
                            output.0
                        }
                        BinaryValueType::Vector => {
                            let output = vector_registers.next();
                            status.insert(
                                expression_id,
                                ExpressionStatus::Done {
                                    register: Register::Vector(output),
                                    dependencies,
                                },
                            );
                            output.0
                        }
                        BinaryValueType::Rgb => {
                            let output = rgb_registers.next();
                            status.insert(
                                expression_id,
                                ExpressionStatus::Done {
                                    register: Register::Rgb(output),
                                    dependencies,
                                },
                            );
                            output.0
                        }
                    };
                    instructions.push(Instruction {
                        instruction_type: InstructionType::Mix {
                            value_type,
                            lhs,
                            rhs,
                            amount,
                            output,
                        },
                        dependencies,
                    });
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

                    let (value_type, (lhs, lhs_deps), (rhs, rhs_deps)) = convert_operands(
                        lhs,
                        rhs,
                        &mut instructions,
                        &mut number_registers,
                        &mut vector_registers,
                        &mut rgb_registers,
                    );

                    let dependencies = lhs_deps | rhs_deps;
                    let output = match value_type {
                        BinaryValueType::Number => {
                            let output = number_registers.next();
                            status.insert(
                                expression_id,
                                ExpressionStatus::Done {
                                    register: Register::Number(output),
                                    dependencies,
                                },
                            );
                            output.0
                        }
                        BinaryValueType::Vector => {
                            let output = vector_registers.next();
                            status.insert(
                                expression_id,
                                ExpressionStatus::Done {
                                    register: Register::Vector(output),
                                    dependencies,
                                },
                            );
                            output.0
                        }
                        BinaryValueType::Rgb => {
                            let output = rgb_registers.next();
                            status.insert(
                                expression_id,
                                ExpressionStatus::Done {
                                    register: Register::Rgb(output),
                                    dependencies,
                                },
                            );
                            output.0
                        }
                    };
                    instructions.push(Instruction {
                        instruction_type: InstructionType::Binary {
                            value_type,
                            operator,
                            lhs,
                            rhs,
                            output,
                        },
                        dependencies,
                    });
                }
                ExpressionStatus::Pending(&ComplexExpression::Clamp { value, min, max }) => {
                    let value = try_get_number_value(
                        value,
                        &mut status,
                        expressions,
                        &mut instructions,
                        &mut number_registers,
                    )?;
                    let (value, value_deps) = unwrap_or_push!(value, expression_id, pending);

                    let min = try_get_number_value(
                        min,
                        &mut status,
                        expressions,
                        &mut instructions,
                        &mut number_registers,
                    )?;
                    let (min, min_deps) = unwrap_or_push!(min, expression_id, pending);

                    let max = try_get_number_value(
                        max,
                        &mut status,
                        expressions,
                        &mut instructions,
                        &mut number_registers,
                    )?;
                    let (max, max_deps) = unwrap_or_push!(max, expression_id, pending);

                    let output = number_registers.next();
                    let dependencies = value_deps | min_deps | max_deps;
                    instructions.push(Instruction {
                        instruction_type: InstructionType::Clamp {
                            value,
                            min,
                            max,
                            output,
                        },
                        dependencies,
                    });
                    status.insert(
                        expression_id,
                        ExpressionStatus::Done {
                            register: Register::Number(output),
                            dependencies,
                        },
                    );
                }
                ExpressionStatus::Done { .. } => {}
            }
        }

        if let Some(ExpressionStatus::Done {
            register,
            dependencies,
        }) = status.remove(&result_id)
        {
            let output = match (register, T::from_value()) {
                (Register::Number(register), ProgramOutputType::Number(convert)) => {
                    ProgramOutput::FromNumber(register, convert)
                }

                (Register::Number(register), ProgramOutputType::Vector(convert)) => {
                    let output = number_register_to_vector(
                        register,
                        dependencies,
                        &mut instructions,
                        &mut vector_registers,
                    );
                    ProgramOutput::FromVector(output, convert)
                }
                (Register::Vector(register), ProgramOutputType::Vector(convert)) => {
                    ProgramOutput::FromVector(register, convert)
                }

                (Register::Rgb(register), ProgramOutputType::Number(convert)) => {
                    let (wavelength, wavelength_deps) = get_number_input(NumberInput::Wavelength)?;

                    let output = number_registers.next();
                    instructions.push(Instruction {
                        instruction_type: InstructionType::RgbSpectrumValue {
                            wavelength,
                            source: register,
                            output,
                        },
                        dependencies: dependencies | wavelength_deps,
                    });
                    ProgramOutput::FromNumber(output, convert)
                }
                (Register::Rgb(register), ProgramOutputType::Vector(convert)) => {
                    let output = rgb_register_to_vector(
                        register,
                        dependencies,
                        &mut instructions,
                        &mut vector_registers,
                    );
                    ProgramOutput::FromVector(output, convert)
                }

                (Register::Vector(_), ProgramOutputType::Number(_)) => {
                    return Err("cannot use a vector as a number".into())
                }
            };

            Ok(Program {
                program_type: ProgramType::Instructions {
                    instructions: self.arena.alloc_slice_copy(&instructions),
                    output,
                    numbers: number_registers.count(),
                    vectors: vector_registers.count(),
                    rgb_values: rgb_registers.count(),
                },
            })
        } else {
            Err("the expression was not compiled to completion".into())
        }
    }
}

enum ExpressionStatus<'a> {
    Pending(&'a ComplexExpression),
    Done {
        register: Register,
        dependencies: Inputs,
    },
}

#[derive(Copy, Clone)]
enum Register {
    Number(NumberRegister),
    Vector(VectorRegister),
    Rgb(RgbRegister),
}

enum NumberOrRegister {
    Number(f32),
    Register {
        register: Register,
        dependencies: Inputs,
    },
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
                ExpressionStatus::Done {
                    register,
                    dependencies,
                } => Ok(NumberOrRegister::Register {
                    register,
                    dependencies,
                }),
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
    number_registers: &mut RegisterCounter<NumberRegister>,
) -> Result<Result<(NumberValue<N>, Inputs), ExpressionId>, Box<dyn Error>>
where
    N: TryFrom<NumberInput, Error = Cow<'static, str>>,
{
    match try_get_register(expression, status, expressions) {
        Ok(NumberOrRegister::Number(number)) => {
            Ok(Ok((NumberValue::Constant(number), Inputs::empty())))
        }
        Ok(NumberOrRegister::Register {
            register: Register::Number(register),
            dependencies,
        }) => Ok(Ok((NumberValue::Register(register), dependencies))),
        Ok(NumberOrRegister::Register {
            register: Register::Vector(_),
            dependencies: _,
        }) => Err("cannot use a vector as a number".into()),
        Ok(NumberOrRegister::Register {
            register: Register::Rgb(rgb_register),
            dependencies,
        }) => {
            let (wavelength, wavelength_deps) = get_number_input(NumberInput::Wavelength)?;

            let output = number_registers.next();
            let dependencies = dependencies | wavelength_deps;
            instructions.push(Instruction {
                instruction_type: InstructionType::RgbSpectrumValue {
                    wavelength,
                    source: rgb_register,
                    output,
                },
                dependencies,
            });
            Ok(Ok((NumberValue::Register(output), dependencies)))
        }
        Err(child_id) => Ok(Err(child_id)),
    }
}

fn convert_operands<N, V>(
    lhs: NumberOrRegister,
    rhs: NumberOrRegister,
    instructions: &mut Vec<Instruction<N, V>>,
    number_registers: &mut RegisterCounter<NumberRegister>,
    vector_registers: &mut RegisterCounter<VectorRegister>,
    rgb_registers: &mut RegisterCounter<RgbRegister>,
) -> (BinaryValueType, (usize, Inputs), (usize, Inputs)) {
    match (lhs, rhs) {
        (NumberOrRegister::Number(lhs), NumberOrRegister::Number(rhs)) => {
            let lhs_output = number_registers.next();
            let rhs_output = number_registers.next();
            instructions.push(Instruction {
                instruction_type: InstructionType::NumberValue {
                    number: lhs,
                    output: lhs_output,
                },
                dependencies: Inputs::empty(),
            });
            instructions.push(Instruction {
                instruction_type: InstructionType::NumberValue {
                    number: rhs,
                    output: rhs_output,
                },
                dependencies: Inputs::empty(),
            });

            (
                BinaryValueType::Number,
                (lhs_output.0, Inputs::empty()),
                (rhs_output.0, Inputs::empty()),
            )
        }
        (
            NumberOrRegister::Number(lhs),
            NumberOrRegister::Register {
                register: Register::Number(rhs),
                dependencies: rhs_deps,
            },
        ) => {
            let lhs_output = number_registers.next();
            instructions.push(Instruction {
                instruction_type: InstructionType::NumberValue {
                    number: lhs,
                    output: lhs_output,
                },
                dependencies: Inputs::empty(),
            });

            (
                BinaryValueType::Number,
                (lhs_output.0, Inputs::empty()),
                (rhs.0, rhs_deps),
            )
        }
        (
            NumberOrRegister::Number(lhs),
            NumberOrRegister::Register {
                register: Register::Rgb(rhs),
                dependencies: rhs_deps,
            },
        ) => {
            let lhs_output = number_constant_to_rgb(lhs, instructions, rgb_registers);

            (
                BinaryValueType::Rgb,
                (lhs_output.0, Inputs::empty()),
                (rhs.0, rhs_deps),
            )
        }
        (
            NumberOrRegister::Number(lhs),
            NumberOrRegister::Register {
                register: Register::Vector(rhs),
                dependencies: rhs_deps,
            },
        ) => {
            let lhs_output = number_constant_to_vector(lhs, instructions, vector_registers);

            (
                BinaryValueType::Vector,
                (lhs_output.0, Inputs::empty()),
                (rhs.0, rhs_deps),
            )
        }
        (
            NumberOrRegister::Register {
                register: Register::Number(lhs),
                dependencies: lhs_deps,
            },
            NumberOrRegister::Number(rhs),
        ) => {
            let rhs_output = number_registers.next();
            instructions.push(Instruction {
                instruction_type: InstructionType::NumberValue {
                    number: rhs,
                    output: rhs_output,
                },
                dependencies: Inputs::empty(),
            });

            (
                BinaryValueType::Number,
                (lhs.0, lhs_deps),
                (rhs_output.0, Inputs::empty()),
            )
        }
        (
            NumberOrRegister::Register {
                register: Register::Rgb(lhs),
                dependencies: lhs_deps,
            },
            NumberOrRegister::Number(rhs),
        ) => {
            let rhs_output = number_constant_to_rgb(rhs, instructions, rgb_registers);

            (
                BinaryValueType::Rgb,
                (lhs.0, lhs_deps),
                (rhs_output.0, Inputs::empty()),
            )
        }
        (
            NumberOrRegister::Register {
                register: Register::Vector(lhs),
                dependencies: lhs_deps,
            },
            NumberOrRegister::Number(rhs),
        ) => {
            let rhs_output = number_constant_to_vector(rhs, instructions, vector_registers);

            (
                BinaryValueType::Vector,
                (lhs.0, lhs_deps),
                (rhs_output.0, Inputs::empty()),
            )
        }
        (
            NumberOrRegister::Register {
                register: Register::Number(lhs),
                dependencies: lhs_deps,
            },
            NumberOrRegister::Register {
                register: Register::Number(rhs),
                dependencies: rhs_deps,
            },
        ) => (
            BinaryValueType::Number,
            (lhs.0, lhs_deps),
            (rhs.0, rhs_deps),
        ),
        (
            NumberOrRegister::Register {
                register: Register::Number(lhs),
                dependencies: lhs_deps,
            },
            NumberOrRegister::Register {
                register: Register::Vector(rhs),
                dependencies: rhs_deps,
            },
        ) => {
            let lhs_output =
                number_register_to_vector(lhs, lhs_deps, instructions, vector_registers);

            (
                BinaryValueType::Vector,
                (lhs_output.0, lhs_deps),
                (rhs.0, rhs_deps),
            )
        }
        (
            NumberOrRegister::Register {
                register: Register::Number(lhs),
                dependencies: lhs_deps,
            },
            NumberOrRegister::Register {
                register: Register::Rgb(rhs),
                dependencies: rhs_deps,
            },
        ) => {
            let lhs_output = number_register_to_rgb(lhs, lhs_deps, instructions, rgb_registers);

            (
                BinaryValueType::Rgb,
                (lhs_output.0, lhs_deps),
                (rhs.0, rhs_deps),
            )
        }
        (
            NumberOrRegister::Register {
                register: Register::Vector(lhs),
                dependencies: lhs_deps,
            },
            NumberOrRegister::Register {
                register: Register::Number(rhs),
                dependencies: rhs_deps,
            },
        ) => {
            let rhs_output =
                number_register_to_vector(rhs, rhs_deps, instructions, vector_registers);

            (
                BinaryValueType::Vector,
                (lhs.0, lhs_deps),
                (rhs_output.0, rhs_deps),
            )
        }
        (
            NumberOrRegister::Register {
                register: Register::Vector(lhs),
                dependencies: lhs_deps,
            },
            NumberOrRegister::Register {
                register: Register::Vector(rhs),
                dependencies: rhs_deps,
            },
        ) => (
            BinaryValueType::Vector,
            (lhs.0, lhs_deps),
            (rhs.0, rhs_deps),
        ),
        (
            NumberOrRegister::Register {
                register: Register::Vector(lhs),
                dependencies: lhs_deps,
            },
            NumberOrRegister::Register {
                register: Register::Rgb(rhs),
                dependencies: rhs_deps,
            },
        ) => {
            let rhs_output = rgb_register_to_vector(rhs, rhs_deps, instructions, vector_registers);

            (
                BinaryValueType::Vector,
                (lhs.0, lhs_deps),
                (rhs_output.0, rhs_deps),
            )
        }
        (
            NumberOrRegister::Register {
                register: Register::Rgb(lhs),
                dependencies: lhs_deps,
            },
            NumberOrRegister::Register {
                register: Register::Number(rhs),
                dependencies: rhs_deps,
            },
        ) => {
            let rhs_output = number_register_to_rgb(rhs, rhs_deps, instructions, rgb_registers);

            (
                BinaryValueType::Rgb,
                (lhs.0, lhs_deps),
                (rhs_output.0, rhs_deps),
            )
        }
        (
            NumberOrRegister::Register {
                register: Register::Rgb(lhs),
                dependencies: lhs_deps,
            },
            NumberOrRegister::Register {
                register: Register::Vector(rhs),
                dependencies: rhs_deps,
            },
        ) => {
            let lhs_output = rgb_register_to_vector(lhs, lhs_deps, instructions, vector_registers);

            (
                BinaryValueType::Vector,
                (lhs_output.0, lhs_deps),
                (rhs.0, rhs_deps),
            )
        }
        (
            NumberOrRegister::Register {
                register: Register::Rgb(lhs),
                dependencies: lhs_deps,
            },
            NumberOrRegister::Register {
                register: Register::Rgb(rhs),
                dependencies: rhs_deps,
            },
        ) => (BinaryValueType::Rgb, (lhs.0, lhs_deps), (rhs.0, rhs_deps)),
    }
}

fn get_number_input<N>(input: NumberInput) -> Result<(NumberValue<N>, Inputs), Box<dyn Error>>
where
    N: TryFrom<NumberInput, Error = Cow<'static, str>>,
{
    Ok((NumberValue::Input(input.try_into()?), input.into()))
}

fn get_vector_input<V>(input: VectorInput) -> Result<(VectorValue<V>, Inputs), Box<dyn Error>>
where
    V: TryFrom<VectorInput, Error = Cow<'static, str>>,
{
    Ok((VectorValue::Input(input.try_into()?), input.into()))
}

fn number_constant_to_vector<N, V>(
    number: f32,
    instructions: &mut Vec<Instruction<N, V>>,
    vector_registers: &mut RegisterCounter<VectorRegister>,
) -> VectorRegister {
    let output = vector_registers.next();
    instructions.push(Instruction {
        instruction_type: InstructionType::VectorValue {
            x: NumberValue::Constant(number),
            y: NumberValue::Constant(number),
            z: NumberValue::Constant(number),
            w: NumberValue::Constant(number),
            output,
        },
        dependencies: Inputs::empty(),
    });
    output
}

fn number_register_to_vector<N, V>(
    register: NumberRegister,
    dependencies: Inputs,
    instructions: &mut Vec<Instruction<N, V>>,
    vector_registers: &mut RegisterCounter<VectorRegister>,
) -> VectorRegister {
    let output = vector_registers.next();
    instructions.push(Instruction {
        instruction_type: InstructionType::VectorValue {
            x: NumberValue::Register(register),
            y: NumberValue::Register(register),
            z: NumberValue::Register(register),
            w: NumberValue::Register(register),
            output,
        },
        dependencies,
    });
    output
}

fn rgb_register_to_vector<N, V>(
    register: RgbRegister,
    dependencies: Inputs,
    instructions: &mut Vec<Instruction<N, V>>,
    vector_registers: &mut RegisterCounter<VectorRegister>,
) -> VectorRegister {
    let output = vector_registers.next();
    instructions.push(Instruction {
        instruction_type: InstructionType::Convert {
            conversion: ValueConversion::RgbToVector {
                source: register,
                output,
            },
        },
        dependencies,
    });
    output
}

fn number_constant_to_rgb<N, V>(
    number: f32,
    instructions: &mut Vec<Instruction<N, V>>,
    rgb_registers: &mut RegisterCounter<RgbRegister>,
) -> RgbRegister {
    let output = rgb_registers.next();
    instructions.push(Instruction {
        instruction_type: InstructionType::RgbValue {
            red: NumberValue::Constant(number),
            green: NumberValue::Constant(number),
            blue: NumberValue::Constant(number),
            output,
        },
        dependencies: Inputs::empty(),
    });
    output
}

fn number_register_to_rgb<N, V>(
    register: NumberRegister,
    dependencies: Inputs,
    instructions: &mut Vec<Instruction<N, V>>,
    rgb_registers: &mut RegisterCounter<RgbRegister>,
) -> RgbRegister {
    let output = rgb_registers.next();
    instructions.push(Instruction {
        instruction_type: InstructionType::RgbValue {
            red: NumberValue::Register(register),
            green: NumberValue::Register(register),
            blue: NumberValue::Register(register),
            output,
        },
        dependencies,
    });
    output
}

struct RegisterCounter<R> {
    next: R,
}

impl<R: RegisterIndex> RegisterCounter<R> {
    fn new() -> Self {
        RegisterCounter { next: R::first() }
    }

    fn next(&mut self) -> R {
        let current = self.next;
        self.next = current.next();
        current
    }

    fn count(&self) -> usize {
        self.next.index()
    }
}

trait RegisterIndex: Copy {
    fn first() -> Self;
    fn next(&self) -> Self;
    fn index(&self) -> usize;
}

impl RegisterIndex for NumberRegister {
    fn first() -> Self {
        NumberRegister(0)
    }
    fn next(&self) -> Self {
        NumberRegister(self.0 + 1)
    }
    fn index(&self) -> usize {
        self.0
    }
}

impl RegisterIndex for VectorRegister {
    fn first() -> Self {
        VectorRegister(0)
    }
    fn next(&self) -> Self {
        VectorRegister(self.0 + 1)
    }
    fn index(&self) -> usize {
        self.0
    }
}

impl RegisterIndex for RgbRegister {
    fn first() -> Self {
        RgbRegister(0)
    }
    fn next(&self) -> Self {
        RgbRegister(self.0 + 1)
    }
    fn index(&self) -> usize {
        self.0
    }
}
