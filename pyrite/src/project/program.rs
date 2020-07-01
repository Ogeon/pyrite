use std::error::Error;

use bumpalo::Bump;

use super::{
    expressions::{BinaryOperator, ComplexExpression, Expression, Expressions, Vector},
    spectra::{Spectra, SpectrumId},
    textures::{TextureId, Textures},
};
use crate::color::Light;
use cgmath::{Point2, Vector3};

pub type ProgramFn<I, T> = for<'a> fn(&'a mut Registers, &'a I, Resources<'a>) -> T;
pub type InputFn<I> = for<'a> fn(&'a mut Registers, &'a I, Resources<'a>) -> Value;

#[derive(Copy, Clone)]
pub struct Resources<'a> {
    pub spectra: &'a Spectra,
    pub textures: &'a Textures,
}

#[derive(Copy, Clone)]
pub struct ProgramCompiler<'p> {
    arena: &'p Bump,
}

impl<'p> ProgramCompiler<'p> {
    pub fn new(arena: &'p Bump) -> Self {
        ProgramCompiler { arena }
    }

    pub fn compile<I, T>(
        &self,
        expression: &Expression,
        expressions: &Expressions,
    ) -> Result<Program<'p, I, T>, Box<dyn Error>>
    where
        I: ProgramInput,
        T: ProgramValue<I> + Into<Value> + 'p,
        AnyProgram<'p, I>: From<Program<'p, I, T>>,
    {
        type CompileProgram<I> = for<'p, 'a> fn(
            ProgramCompiler<'p>,
            &'a Expression,
            &'a Expressions,
        )
            -> Result<AnyProgram<'p, I>, Box<dyn Error>>;

        enum StackEntry<'e, I, T> {
            Expression(&'e Expression),
            Program(&'e Expression, CompileProgram<I>),
            Function(ProgramFn<I, T>),
            Number(f64),
        }

        let mut instructions = Vec::new();
        let mut stack = vec![StackEntry::Expression(expression)];

        while let Some(entry) = stack.pop() {
            let id = match entry {
                StackEntry::Expression(&Expression::Complex(id)) => id,
                StackEntry::Expression(&Expression::Number(number)) => {
                    instructions.push(Instruction::Push(T::from_number(number as f32)?.into()));
                    continue;
                }
                StackEntry::Program(expression, compile) => {
                    instructions.push(Instruction::Program(compile(
                        *self,
                        expression,
                        expressions,
                    )?));
                    continue;
                }
                StackEntry::Function(function) => {
                    instructions.push(Instruction::Function(function));
                    continue;
                }
                StackEntry::Number(number) => {
                    instructions.push(Instruction::Push(Value::Number(number as f32)));
                    continue;
                }
            };

            match expressions.get(id) {
                ComplexExpression::Vector { x, y, z, w } => {
                    if let Some((x, y, z, w)) = into_constant_vector(x, y, z, w) {
                        instructions.push(Instruction::Push(T::from_vector(x, y, z, w)?.into()));
                    } else {
                        if let Some(vector) = T::vector()? {
                            stack.push(StackEntry::Function(vector));
                        }
                        match w {
                            Expression::Number(number) => stack.push(StackEntry::Number(*number)),
                            other => stack.push(StackEntry::Program(
                                other,
                                |this, expression, expressions| {
                                    this.compile_any::<I, f32>(expression, expressions)
                                },
                            )),
                        }
                        match z {
                            Expression::Number(number) => stack.push(StackEntry::Number(*number)),
                            other => stack.push(StackEntry::Program(
                                other,
                                |this, expression, expressions| {
                                    this.compile_any::<I, f32>(expression, expressions)
                                },
                            )),
                        }
                        match y {
                            Expression::Number(number) => stack.push(StackEntry::Number(*number)),
                            other => stack.push(StackEntry::Program(
                                other,
                                |this, expression, expressions| {
                                    this.compile_any::<I, f32>(expression, expressions)
                                },
                            )),
                        }
                        match x {
                            Expression::Number(number) => stack.push(StackEntry::Number(*number)),
                            other => stack.push(StackEntry::Program(
                                other,
                                |this, expression, expressions| {
                                    this.compile_any::<I, f32>(expression, expressions)
                                },
                            )),
                        }
                    }
                }
                ComplexExpression::Rgb { red, green, blue } => {
                    if let Some(rgb) = T::rgb()? {
                        stack.push(StackEntry::Function(rgb));
                    }
                    match blue {
                        Expression::Number(number) => stack.push(StackEntry::Number(*number)),
                        other => stack.push(StackEntry::Program(
                            other,
                            |this, expression, expressions| {
                                this.compile_any::<I, f32>(expression, expressions)
                            },
                        )),
                    }
                    match green {
                        Expression::Number(number) => stack.push(StackEntry::Number(*number)),
                        other => stack.push(StackEntry::Program(
                            other,
                            |this, expression, expressions| {
                                this.compile_any::<I, f32>(expression, expressions)
                            },
                        )),
                    }
                    match red {
                        Expression::Number(number) => stack.push(StackEntry::Number(*number)),
                        other => stack.push(StackEntry::Program(
                            other,
                            |this, expression, expressions| {
                                this.compile_any::<I, f32>(expression, expressions)
                            },
                        )),
                    }
                }
                ComplexExpression::Binary { operator, lhs, rhs } => {
                    let operator = match operator {
                        BinaryOperator::Add => T::add()?,
                        BinaryOperator::Sub => T::sub()?,
                        BinaryOperator::Mul => T::mul()?,
                        BinaryOperator::Div => T::div()?,
                    };
                    stack.push(StackEntry::Function(operator));
                    stack.push(StackEntry::Expression(rhs));
                    stack.push(StackEntry::Expression(lhs));
                }
                ComplexExpression::Mix { amount, lhs, rhs } => {
                    stack.push(StackEntry::Function(T::mix()?));
                    match amount {
                        Expression::Number(number) => stack.push(StackEntry::Number(*number)),
                        other => stack.push(StackEntry::Program(
                            other,
                            |this, expression, expressions| {
                                this.compile_any::<I, f32>(expression, expressions)
                            },
                        )),
                    }
                    stack.push(StackEntry::Expression(rhs));
                    stack.push(StackEntry::Expression(lhs));
                }
                ComplexExpression::Fresnel { ior, env_ior } => {
                    stack.push(StackEntry::Function(T::fresnel()?));
                    instructions.push(Instruction::Input(I::normal()?));
                    instructions.push(Instruction::Input(I::incident()?));
                    match env_ior {
                        Expression::Number(number) => stack.push(StackEntry::Number(*number)),
                        other => stack.push(StackEntry::Program(
                            other,
                            |this, expression, expressions| {
                                this.compile_any::<I, f32>(expression, expressions)
                            },
                        )),
                    }
                    match ior {
                        Expression::Number(number) => stack.push(StackEntry::Number(*number)),
                        other => stack.push(StackEntry::Program(
                            other,
                            |this, expression, expressions| {
                                this.compile_any::<I, f32>(expression, expressions)
                            },
                        )),
                    }
                }
                ComplexExpression::Spectrum { points } => {
                    instructions.push(Instruction::Push(Value::Spectrum(*points)));
                    if let Some(spectrum) = T::spectrum()? {
                        instructions.push(Instruction::Function(spectrum));
                    }
                }
                ComplexExpression::Texture { texture } => {
                    instructions.push(Instruction::Push(Value::Texture(*texture)));
                    instructions.push(Instruction::Input(I::texture_coordinates()?));
                    if let Some(texture) = T::texture()? {
                        instructions.push(Instruction::Function(texture));
                    }
                }
            }
        }

        Ok(Program {
            instructions: self.arena.alloc_slice_copy(&instructions),
        })
    }

    fn compile_any<I, T>(
        &self,
        expression: &Expression,
        expressions: &Expressions,
    ) -> Result<AnyProgram<'p, I>, Box<dyn Error>>
    where
        I: ProgramInput,
        T: ProgramValue<I> + Into<Value> + 'p,
        AnyProgram<'p, I>: From<Program<'p, I, T>>,
    {
        Ok(self.compile(expression, expressions)?.into())
    }
}

fn into_constant_vector(
    x: &Expression,
    y: &Expression,
    z: &Expression,
    w: &Expression,
) -> Option<(f32, f32, f32, f32)> {
    match (x, y, z, w) {
        (
            &Expression::Number(x),
            &Expression::Number(y),
            &Expression::Number(z),
            &Expression::Number(w),
        ) => Some((x as f32, y as f32, z as f32, w as f32)),
        _ => None,
    }
}

pub trait ProgramValue<I>: Copy + Send + Sized {
    fn from_number(number: f32) -> Result<Self, Box<dyn Error>>;
    fn from_vector(x: f32, y: f32, z: f32, w: f32) -> Result<Self, Box<dyn Error>>;
    fn number() -> Result<Option<ProgramFn<I, Self>>, Box<dyn Error>>;
    fn vector() -> Result<Option<ProgramFn<I, Self>>, Box<dyn Error>>;
    fn rgb() -> Result<Option<ProgramFn<I, Self>>, Box<dyn Error>>;
    fn spectrum() -> Result<Option<ProgramFn<I, Self>>, Box<dyn Error>>;
    fn texture() -> Result<Option<ProgramFn<I, Self>>, Box<dyn Error>>;
    fn add() -> Result<ProgramFn<I, Self>, Box<dyn Error>>;
    fn sub() -> Result<ProgramFn<I, Self>, Box<dyn Error>>;
    fn mul() -> Result<ProgramFn<I, Self>, Box<dyn Error>>;
    fn div() -> Result<ProgramFn<I, Self>, Box<dyn Error>>;
    fn mix() -> Result<ProgramFn<I, Self>, Box<dyn Error>>;
    fn fresnel() -> Result<ProgramFn<I, Self>, Box<dyn Error>>;
}

impl<I> ProgramValue<I> for f32 {
    fn from_number(number: f32) -> Result<Self, Box<dyn Error>> {
        Ok(number)
    }
    fn from_vector(_x: f32, _y: f32, _z: f32, _w: f32) -> Result<Self, Box<dyn Error>> {
        Err("vectors cannot be used as numbers".into())
    }
    fn number() -> Result<Option<ProgramFn<I, Self>>, Box<dyn Error>> {
        Ok(None)
    }
    fn vector() -> Result<Option<ProgramFn<I, Self>>, Box<dyn Error>> {
        Err("vectors cannot be used as numbers".into())
    }
    fn rgb() -> Result<Option<ProgramFn<I, Self>>, Box<dyn Error>> {
        Err("RGB colors cannot be used as numbers".into())
    }
    fn spectrum() -> Result<Option<ProgramFn<I, Self>>, Box<dyn Error>> {
        Err("spectra cannot be used as numbers".into())
    }
    fn texture() -> Result<Option<ProgramFn<I, Self>>, Box<dyn Error>> {
        Ok(Some(|registers, _, resources| {
            let texture = resources.textures.get(registers.pop());
            let uv: Vector = registers.pop();

            texture.get_color(uv.into()).red
        }))
    }
    fn add() -> Result<ProgramFn<I, Self>, Box<dyn Error>> {
        Ok(|registers, _, _| {
            let rhs: f32 = registers.pop();
            let lhs: f32 = registers.pop();
            lhs + rhs
        })
    }
    fn sub() -> Result<ProgramFn<I, Self>, Box<dyn Error>> {
        Ok(|registers, _, _| {
            let rhs: f32 = registers.pop();
            let lhs: f32 = registers.pop();
            lhs - rhs
        })
    }
    fn mul() -> Result<ProgramFn<I, Self>, Box<dyn Error>> {
        Ok(|registers, _, _| {
            let rhs: f32 = registers.pop();
            let lhs: f32 = registers.pop();
            lhs * rhs
        })
    }
    fn div() -> Result<ProgramFn<I, Self>, Box<dyn Error>> {
        Ok(|registers, _, _| {
            let rhs: f32 = registers.pop();
            let lhs: f32 = registers.pop();
            lhs / rhs
        })
    }
    fn mix() -> Result<ProgramFn<I, Self>, Box<dyn Error>> {
        Ok(|registers, _, _| {
            let amount = registers.pop::<f32>().min(1.0).max(0.0);
            let rhs: f32 = registers.pop();
            let lhs: f32 = registers.pop();
            lhs * (1.0 - amount) + rhs * amount
        })
    }
    fn fresnel() -> Result<ProgramFn<I, Self>, Box<dyn Error>> {
        Ok(|registers, _, _| {
            let incident: Vector = registers.pop();
            let normal: Vector = registers.pop();
            let env_ior: f32 = registers.pop();
            let ior: f32 = registers.pop();
            crate::math::fresnel(ior, env_ior, normal.into(), incident.into())
        })
    }
}

pub trait ProgramInput {
    fn normal() -> Result<InputFn<Self>, Box<dyn Error>>;
    fn incident() -> Result<InputFn<Self>, Box<dyn Error>>;
    fn texture_coordinates() -> Result<InputFn<Self>, Box<dyn Error>>;
}

pub struct Program<'p, I, T> {
    instructions: &'p [Instruction<'p, I, T>],
}

impl<'p, I, T> Clone for Program<'p, I, T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<'p, I, T> Copy for Program<'p, I, T> {}

pub enum Instruction<'p, I, T> {
    Push(Value),
    Input(InputFn<I>),
    Function(ProgramFn<I, T>),
    Program(AnyProgram<'p, I>),
}

impl<'p, I, T: Copy> Clone for Instruction<'p, I, T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<'p, I, T: Copy> Copy for Instruction<'p, I, T> {}

#[derive(Copy, Clone)]
pub enum Value {
    Number(f32),
    Spectrum(SpectrumId),
    Texture(TextureId),
    Vector(Vector),
}

impl Value {
    fn push(self, registers: &mut Registers) {
        match self {
            Value::Number(number) => number.push(registers),
            Value::Spectrum(spectrum) => spectrum.push(registers),
            Value::Texture(texture) => texture.push(registers),
            Value::Vector(vector) => vector.push(registers),
        }
    }
}

impl From<f32> for Value {
    fn from(number: f32) -> Self {
        Value::Number(number)
    }
}

impl From<Light> for Value {
    fn from(light: Light) -> Self {
        Value::Number(light.value)
    }
}

impl From<SpectrumId> for Value {
    fn from(spectrum: SpectrumId) -> Self {
        Value::Spectrum(spectrum)
    }
}

impl From<Point2<f32>> for Value {
    fn from(vector: Point2<f32>) -> Self {
        Value::Vector(vector.into())
    }
}

impl From<Vector3<f32>> for Value {
    fn from(vector: Vector3<f32>) -> Self {
        Value::Vector(vector.into())
    }
}

pub enum AnyProgram<'p, I> {
    Number(Program<'p, I, f32>),
    Vector(Program<'p, I, Vector>),
    Light(Program<'p, I, Light>),
}

impl<'p, I> Clone for AnyProgram<'p, I> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<'p, I> Copy for AnyProgram<'p, I> {}

impl<'p, I> From<Program<'p, I, f32>> for AnyProgram<'p, I> {
    fn from(program: Program<'p, I, f32>) -> Self {
        AnyProgram::Number(program)
    }
}

impl<'p, I> From<Program<'p, I, Vector>> for AnyProgram<'p, I> {
    fn from(program: Program<'p, I, Vector>) -> Self {
        AnyProgram::Vector(program)
    }
}

impl<'p, I> From<Program<'p, I, Light>> for AnyProgram<'p, I> {
    fn from(program: Program<'p, I, Light>) -> Self {
        AnyProgram::Light(program)
    }
}

pub struct ExecutionContext<'p> {
    registers: Registers,
    resources: Resources<'p>,
}

impl<'p> ExecutionContext<'p> {
    pub fn new(resources: Resources<'p>) -> Self {
        ExecutionContext {
            registers: Registers::new(),
            resources,
        }
    }

    #[inline]
    pub fn run<I, T>(&mut self, program: Program<'p, I, T>, input: &I) -> T
    where
        T: RegisterValue,
        AnyProgram<'p, I>: From<Program<'p, I, T>>,
    {
        self.registers.clear();

        self.run_program(program, input);

        self.registers.pop()
    }

    fn run_any_program<I>(&mut self, program: AnyProgram<'p, I>, input: &I) {
        match program {
            AnyProgram::Number(program) => self.run_program(program, input),
            AnyProgram::Vector(program) => self.run_program(program, input),
            AnyProgram::Light(program) => self.run_program(program, input),
        }
    }

    #[inline]
    fn run_program<I, T>(&mut self, program: Program<'p, I, T>, input: &I)
    where
        T: RegisterValue,
        AnyProgram<'p, I>: From<Program<'p, I, T>>,
    {
        for instruction in program.instructions.iter() {
            match instruction {
                Instruction::Push(value) => value.clone().push(&mut self.registers),
                Instruction::Input(function) => {
                    let result = function(&mut self.registers, input, self.resources);
                    result.push(&mut self.registers);
                }
                Instruction::Function(function) => {
                    let result = function(&mut self.registers, input, self.resources);
                    result.push(&mut self.registers);
                }
                Instruction::Program(next_program) => {
                    self.run_any_program(*next_program, input);
                }
            }
        }
    }
}

pub struct Registers {
    numbers: Vec<f32>,
    vectors: Vec<Vector>,
    spectra: Vec<SpectrumId>,
    textures: Vec<TextureId>,
}

impl Registers {
    fn new() -> Self {
        Registers {
            numbers: Vec::with_capacity(100),
            vectors: Vec::with_capacity(100),
            spectra: Vec::with_capacity(100),
            textures: Vec::with_capacity(100),
        }
    }

    pub fn get<T: RegisterValue>(&self, index: usize) -> T {
        T::get(self, index)
    }

    pub fn pop<T: RegisterValue>(&mut self) -> T {
        T::pop(self)
    }

    fn clear(&mut self) {
        self.numbers.clear();
        self.vectors.clear();
        self.spectra.clear();
        self.textures.clear();
    }
}

pub trait RegisterValue: Clone {
    fn push(self, registers: &mut Registers);
    fn get(registers: &Registers, index: usize) -> Self;
    fn pop(registers: &mut Registers) -> Self;
}

impl RegisterValue for f32 {
    fn push(self, registers: &mut Registers) {
        registers.numbers.push(self);
    }
    fn get(registers: &Registers, index: usize) -> Self {
        registers.numbers[index]
    }
    fn pop(registers: &mut Registers) -> Self {
        registers.numbers.pop().unwrap()
    }
}

impl RegisterValue for Vector {
    fn push(self, registers: &mut Registers) {
        registers.vectors.push(self);
    }
    fn get(registers: &Registers, index: usize) -> Self {
        registers.vectors[index]
    }
    fn pop(registers: &mut Registers) -> Self {
        registers.vectors.pop().unwrap()
    }
}

impl RegisterValue for Light {
    fn push(self, registers: &mut Registers) {
        registers.numbers.push(self.value);
    }
    fn get(registers: &Registers, index: usize) -> Self {
        Light {
            value: registers.numbers[index],
        }
    }
    fn pop(registers: &mut Registers) -> Self {
        Light {
            value: registers.numbers.pop().unwrap(),
        }
    }
}

impl RegisterValue for SpectrumId {
    fn push(self, registers: &mut Registers) {
        registers.spectra.push(self);
    }
    fn get(registers: &Registers, index: usize) -> Self {
        registers.spectra[index]
    }
    fn pop(registers: &mut Registers) -> Self {
        registers.spectra.pop().unwrap()
    }
}

impl RegisterValue for TextureId {
    fn push(self, registers: &mut Registers) {
        registers.textures.push(self);
    }
    fn get(registers: &Registers, index: usize) -> Self {
        registers.textures[index]
    }
    fn pop(registers: &mut Registers) -> Self {
        registers.textures.pop().unwrap()
    }
}
