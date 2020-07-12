use std::{
    collections::{hash_map::Entry, HashMap},
    error::Error,
    ops::{Add, Div, Mul, Sub},
};

use cgmath::{
    ElementWise, EuclideanSpace, Point2, Point3, Quaternion, Vector2, Vector3, Vector4, VectorSpace,
};

use palette::LinSrgb;

use crate::{light_source, texture::ColorEncoding};

use super::{
    eval_context::{EvalContext, Evaluate},
    parse_context::{Parse, ParseContext},
    program::{ProgramFn, ProgramValue},
    spectra::{Spectrum, SpectrumId},
    tables::{TableExt, TableId},
    textures::TextureId,
};

pub struct Expressions {
    expressions: Vec<ComplexExpression>,
}

impl Expressions {
    pub fn get(&self, id: ExpressionId) -> &ComplexExpression {
        self.expressions.get(id.0).expect("missing expression")
    }
}

pub struct ExpressionLoader<'lua> {
    expressions: Vec<ExpressionEntry<'lua>>,
    table_map: HashMap<TableId, ExpressionId>,
    pending: Vec<ExpressionId>,
}

impl<'lua> ExpressionLoader<'lua> {
    pub fn new() -> Self {
        ExpressionLoader {
            expressions: Vec::new(),
            table_map: HashMap::new(),
            pending: Vec::new(),
        }
    }

    pub fn insert(&mut self, table: rlua::Table<'lua>) -> Result<ExpressionId, Box<dyn Error>> {
        let table_id = table.get_id()?;

        match self.table_map.entry(table_id) {
            Entry::Occupied(entry) => Ok(*entry.get()),
            Entry::Vacant(entry) => {
                let id = ExpressionId(self.expressions.len());
                self.expressions.push(ExpressionEntry::Pending(table));
                entry.insert(id);
                self.pending.push(id);
                Ok(id)
            }
        }
    }

    pub fn next_pending(&mut self) -> Option<(ExpressionId, rlua::Table<'lua>)> {
        self.pending.pop().map(|id| {
            let table = self.expressions[id.0].expect_pending();
            (id, table.clone())
        })
    }

    pub fn replace_pending(&mut self, id: ExpressionId, expression: ComplexExpression) {
        self.expressions[id.0] = ExpressionEntry::Parsed(expression);
    }

    pub fn into_expressions(self) -> Expressions {
        Expressions {
            expressions: self
                .expressions
                .into_iter()
                .map(ExpressionEntry::into_parsed)
                .collect(),
        }
    }
}

enum ExpressionEntry<'lua> {
    Parsed(ComplexExpression),
    Pending(rlua::Table<'lua>),
}

impl<'lua> ExpressionEntry<'lua> {
    fn into_parsed(self) -> ComplexExpression {
        if let ExpressionEntry::Parsed(expression) = self {
            expression
        } else {
            panic!("some expressions were not parsed")
        }
    }

    fn expect_pending(&self) -> &rlua::Table<'lua> {
        if let ExpressionEntry::Pending(table) = self {
            table
        } else {
            panic!("expected expression to still be unparsed")
        }
    }
}

#[derive(Copy, Clone)]
pub enum Expression {
    Number(f64),
    Complex(ExpressionId),
}

impl<'lua> Parse<'lua> for Expression {
    type Input = rlua::Value<'lua>;

    fn parse<'a>(context: ParseContext<'a, 'lua, Self::Input>) -> Result<Self, Box<dyn Error>> {
        if let Ok(number) = context.expect_number() {
            return Ok(Expression::Number(number));
        }

        let table = if let Ok(table) = context.expect_table() {
            table
        } else {
            return Err(format!(
                "expected a number or a table but found {:?}",
                context.value()
            )
            .into());
        };

        let id = context.expressions.insert(table)?;
        Ok(Expression::Complex(id))
    }
}

impl<T: ExpressionValue> Evaluate<T> for Expression {
    fn evaluate<'a>(&self, context: EvalContext<'a>) -> Result<T, Box<dyn Error>> {
        match *self {
            Expression::Number(number) => T::from_number(number),
            Expression::Complex(id) => context.expressions.get(id).evaluate(context),
        }
    }
}

impl Evaluate<Point3<f32>> for Expression {
    fn evaluate<'a>(&self, context: EvalContext<'a>) -> Result<Point3<f32>, Box<dyn Error>> {
        let vector = match *self {
            Expression::Number(number) => <Vector as ExpressionValue>::from_number(number),
            Expression::Complex(id) => context.expressions.get(id).evaluate(context),
        };

        Ok(vector?.into())
    }
}

impl Evaluate<Vector3<f32>> for Expression {
    fn evaluate<'a>(&self, context: EvalContext<'a>) -> Result<Vector3<f32>, Box<dyn Error>> {
        let vector = match *self {
            Expression::Number(number) => <Vector as ExpressionValue>::from_number(number),
            Expression::Complex(id) => context.expressions.get(id).evaluate(context),
        };

        Ok(vector?.into())
    }
}

impl Evaluate<Vector2<f32>> for Expression {
    fn evaluate<'a>(&self, context: EvalContext<'a>) -> Result<Vector2<f32>, Box<dyn Error>> {
        let vector = match *self {
            Expression::Number(number) => <Vector as ExpressionValue>::from_number(number),
            Expression::Complex(id) => context.expressions.get(id).evaluate(context),
        };

        Ok(vector?.into())
    }
}

impl Evaluate<Quaternion<f32>> for Expression {
    fn evaluate<'a>(&self, context: EvalContext<'a>) -> Result<Quaternion<f32>, Box<dyn Error>> {
        let vector = match *self {
            Expression::Number(number) => <Vector as ExpressionValue>::from_number(number),
            Expression::Complex(id) => context.expressions.get(id).evaluate(context),
        };

        Ok(vector?.into())
    }
}

pub enum ComplexExpression {
    Vector {
        x: Expression,
        y: Expression,
        z: Expression,
        w: Expression,
    },
    Rgb {
        red: Expression,
        green: Expression,
        blue: Expression,
    },
    Binary {
        operator: BinaryOperator,
        lhs: Expression,
        rhs: Expression,
    },
    Mix {
        amount: Expression,
        lhs: Expression,
        rhs: Expression,
    },
    Fresnel {
        ior: Expression,
        env_ior: Expression,
    },
    Blackbody {
        temperature: Expression,
    },
    Spectrum {
        points: SpectrumId,
    },
    Texture {
        texture: TextureId,
    },
    DebugNormal,
}

impl<'lua> Parse<'lua> for ComplexExpression {
    type Input = rlua::Table<'lua>;

    fn parse<'a>(mut context: ParseContext<'a, 'lua, Self::Input>) -> Result<Self, Box<dyn Error>> {
        let expression_type = context.expect_field::<String>("type")?;

        match &*expression_type {
            "vector" => Ok(ComplexExpression::Vector {
                x: context.parse_field("x")?,
                y: context.parse_field("y")?,
                z: context.parse_field("z")?,
                w: context.parse_field("w")?,
            }),
            "rgb" => Ok(ComplexExpression::Rgb {
                red: context.parse_field("red")?,
                green: context.parse_field("green")?,
                blue: context.parse_field("blue")?,
            }),
            "binary" => Ok(ComplexExpression::Binary {
                operator: context.parse_field("operator")?,
                lhs: context.parse_field("lhs")?,
                rhs: context.parse_field("rhs")?,
            }),
            "mix" => Ok(ComplexExpression::Mix {
                amount: context.parse_field("amount")?,
                lhs: context.parse_field("lhs")?,
                rhs: context.parse_field("rhs")?,
            }),
            "fresnel" => Ok(ComplexExpression::Fresnel {
                ior: context.parse_field("ior")?,
                env_ior: context.parse_field("env_ior")?,
            }),
            "blackbody" => Ok(ComplexExpression::Blackbody {
                temperature: context.parse_field("temperature")?,
            }),
            "spectrum" => {
                let id = context.value().get_id()?;
                let points = if let Some(points) = context.spectra.get(id) {
                    points
                } else {
                    let spectrum = Spectrum::parse(context.clone())?;
                    context.spectra.insert(id, spectrum)
                };

                Ok(ComplexExpression::Spectrum { points })
            }
            "light_source" => {
                let id = context.value().get_id()?;
                let points = if let Some(points) = context.spectra.get(id) {
                    points
                } else {
                    let name: String = context.expect_field("name")?;
                    let spectrum = match &*name {
                        "a" => light_source::A,
                        "d65" => light_source::D65,
                        _ => return Err(format!("unknown builtin spectrum: {}", name).into()),
                    };
                    context.spectra.insert(id, spectrum)
                };

                Ok(ComplexExpression::Spectrum { points })
            }
            "texture" => {
                let encoding = match &*context.expect_field::<String>("encoding")? {
                    "linear" => ColorEncoding::Linear,
                    "srgb" => ColorEncoding::Srgb,
                    encoding => Err(format!("unknown color encoding: {}", encoding))?,
                };

                Ok(ComplexExpression::Texture {
                    texture: context
                        .textures
                        .load(context.expect_field::<String>("path")?, encoding)?,
                })
            }
            "debug_normal" => Ok(ComplexExpression::DebugNormal),
            name => Err(format!("unexpected expression type: '{}'", name).into()),
        }
    }
}

impl<T: ExpressionValue> Evaluate<T> for ComplexExpression {
    fn evaluate<'a>(&self, context: EvalContext<'a>) -> Result<T, Box<dyn Error>> {
        match *self {
            ComplexExpression::Vector { x, y, z, w } => {
                let x: f32 = x.evaluate(context)?;
                let y: f32 = y.evaluate(context)?;
                let z: f32 = z.evaluate(context)?;
                let w: f32 = w.evaluate(context)?;

                T::from_vector(x, y, z, w)
            }
            ComplexExpression::Rgb { red, green, blue } => {
                let red: f32 = red.evaluate(context)?;
                let green: f32 = green.evaluate(context)?;
                let blue: f32 = blue.evaluate(context)?;

                T::from_rgb(red, green, blue)
            }
            ComplexExpression::Binary { operator, lhs, rhs } => {
                let lhs: T = lhs.evaluate(context)?;
                let rhs: T = rhs.evaluate(context)?;

                match operator {
                    BinaryOperator::Add => Ok(lhs + rhs),
                    BinaryOperator::Sub => Ok(lhs - rhs),
                    BinaryOperator::Mul => Ok(lhs * rhs),
                    BinaryOperator::Div => Ok(lhs / rhs),
                }
            }
            ComplexExpression::Mix { amount, lhs, rhs } => {
                let amount: f32 = amount.evaluate(context)?;
                let lhs: T = lhs.evaluate(context)?;
                let rhs: T = rhs.evaluate(context)?;
                T::mix(lhs, rhs, amount)
            }
            ComplexExpression::Fresnel { .. } => {
                Err("cannot evaluate Fresnel functions as constants".into())
            }
            ComplexExpression::Blackbody { .. } => {
                Err("cannot evaluate black-body functions as constants".into())
            }
            ComplexExpression::Spectrum { .. } => {
                Err("cannot evaluate spectra as constants".into())
            }
            ComplexExpression::Texture { .. } => {
                Err("cannot evaluate textures as constants".into())
            }
            ComplexExpression::DebugNormal { .. } => {
                Err("cannot evaluate surface normals as constants".into())
            }
        }
    }
}

#[derive(Copy, Clone)]
pub enum BinaryOperator {
    Add,
    Sub,
    Mul,
    Div,
}

impl<'lua> Parse<'lua> for BinaryOperator {
    type Input = String;

    fn parse<'a>(context: ParseContext<'a, 'lua, Self::Input>) -> Result<Self, Box<dyn Error>> {
        let operator = context.value();

        match &**operator {
            "add" => Ok(BinaryOperator::Add),
            "sub" => Ok(BinaryOperator::Sub),
            "mul" => Ok(BinaryOperator::Mul),
            "div" => Ok(BinaryOperator::Div),
            name => Err(format!("unexpected binary operator: '{}'", name).into()),
        }
    }
}

pub trait ExpressionValue:
    Sized + Add<Output = Self> + Sub<Output = Self> + Mul<Output = Self> + Div<Output = Self>
{
    fn from_number(number: f64) -> Result<Self, Box<dyn Error>>;
    fn from_vector(x: f32, y: f32, z: f32, w: f32) -> Result<Self, Box<dyn Error>>;
    fn from_rgb(red: f32, green: f32, blue: f32) -> Result<Self, Box<dyn Error>>;
    fn mix(lhs: Self, rhs: Self, amount: f32) -> Result<Self, Box<dyn Error>>;
}

impl ExpressionValue for f32 {
    fn from_number(number: f64) -> Result<Self, Box<dyn Error>> {
        Ok(number as f32)
    }

    fn from_vector(_x: f32, _y: f32, _z: f32, _w: f32) -> Result<Self, Box<dyn Error>> {
        Err("expected a number, but found a vector".into())
    }

    fn from_rgb(_red: f32, _green: f32, _blue: f32) -> Result<Self, Box<dyn Error>> {
        Err("expected a number, but found an RGB color".into())
    }

    fn mix(lhs: Self, rhs: Self, amount: f32) -> Result<Self, Box<dyn Error>> {
        let amount = amount.min(1.0).max(0.0);
        Ok(lhs * (1.0 - amount) + rhs * amount)
    }
}

impl ExpressionValue for u16 {
    fn from_number(number: f64) -> Result<Self, Box<dyn Error>> {
        Ok(number as u16)
    }

    fn from_vector(_x: f32, _y: f32, _z: f32, _w: f32) -> Result<Self, Box<dyn Error>> {
        Err("expected a number, but found a vector".into())
    }

    fn from_rgb(_red: f32, _green: f32, _blue: f32) -> Result<Self, Box<dyn Error>> {
        Err("expected a number, but found an RGB color".into())
    }

    fn mix(lhs: Self, rhs: Self, amount: f32) -> Result<Self, Box<dyn Error>> {
        Ok(<f32 as ExpressionValue>::mix(lhs as f32, rhs as f32, amount)? as u16)
    }
}

#[derive(Copy, Clone)]
#[repr(transparent)]
pub struct Vector(Vector4<f32>);

impl ExpressionValue for Vector {
    fn from_number(number: f64) -> Result<Self, Box<dyn Error>> {
        Ok(Vector(Vector4::new(
            number as f32,
            number as f32,
            number as f32,
            number as f32,
        )))
    }

    fn from_vector(x: f32, y: f32, z: f32, w: f32) -> Result<Self, Box<dyn Error>> {
        Ok(Vector(Vector4::new(x, y, z, w)))
    }

    fn from_rgb(_red: f32, _green: f32, _blue: f32) -> Result<Self, Box<dyn Error>> {
        Err("expected a vector, but found an RGB color".into())
    }

    fn mix(lhs: Self, rhs: Self, amount: f32) -> Result<Self, Box<dyn Error>> {
        Ok(Vector(lhs.0.lerp(rhs.0, amount.min(1.0).max(0.0))))
    }
}

impl<I> ProgramValue<I> for Vector {
    fn from_number(number: f32) -> Result<Self, Box<dyn Error>> {
        Ok(Vector(Vector4::new(number, number, number, number)))
    }
    fn from_vector(x: f32, y: f32, z: f32, w: f32) -> Result<Self, Box<dyn Error>> {
        Ok(Vector(Vector4::new(x, y, z, w)))
    }
    fn number() -> Result<Option<ProgramFn<I, Self>>, Box<dyn Error>> {
        Ok(Some(|registers, _, _| {
            let number = registers.pop();
            Vector(Vector4::new(number, number, number, number))
        }))
    }
    fn vector() -> Result<Option<ProgramFn<I, Self>>, Box<dyn Error>> {
        Ok(None)
    }
    fn rgb() -> Result<Option<ProgramFn<I, Self>>, Box<dyn Error>> {
        Ok(Some(|registers, _, _| {
            let blue: f32 = registers.pop();
            let green: f32 = registers.pop();
            let red: f32 = registers.pop();

            let x = (red * 2.0) - 1.0;
            let y = (green * 2.0) - 1.0;
            let z = (blue * 2.0) - 1.0;

            Vector(Vector4::new(x, y, z, 0.0))
        }))
    }
    fn spectrum() -> Result<Option<ProgramFn<I, Self>>, Box<dyn Error>> {
        Err("spectra cannot be used as vectors".into())
    }
    fn texture() -> Result<Option<ProgramFn<I, Self>>, Box<dyn Error>> {
        Ok(Some(|registers, _, resources| {
            let texture = resources.textures.get(registers.pop());
            let uv: Vector = registers.pop();

            let LinSrgb {
                red, green, blue, ..
            } = texture.get_color(uv.into()).color;

            let x = (red * 2.0) - 1.0;
            let y = (green * 2.0) - 1.0;
            let z = (blue * 2.0) - 1.0;

            Vector(Vector4::new(x, y, z, 0.0))
        }))
    }
    fn add() -> Result<ProgramFn<I, Self>, Box<dyn Error>> {
        Ok(|registers, _, _| {
            let rhs: Vector = registers.pop();
            let lhs: Vector = registers.pop();
            lhs + rhs
        })
    }
    fn sub() -> Result<ProgramFn<I, Self>, Box<dyn Error>> {
        Ok(|registers, _, _| {
            let rhs: Vector = registers.pop();
            let lhs: Vector = registers.pop();
            lhs - rhs
        })
    }
    fn mul() -> Result<ProgramFn<I, Self>, Box<dyn Error>> {
        Ok(|registers, _, _| {
            let rhs: Vector = registers.pop();
            let lhs: Vector = registers.pop();
            lhs * rhs
        })
    }
    fn div() -> Result<ProgramFn<I, Self>, Box<dyn Error>> {
        Ok(|registers, _, _| {
            let rhs: Vector = registers.pop();
            let lhs: Vector = registers.pop();
            lhs / rhs
        })
    }
    fn mix() -> Result<ProgramFn<I, Self>, Box<dyn Error>> {
        Ok(|registers, _, _| {
            let amount = registers.pop::<f32>().min(1.0).max(0.0);
            let rhs: Vector = registers.pop();
            let lhs: Vector = registers.pop();
            Vector(lhs.0.lerp(rhs.0, amount))
        })
    }
    fn fresnel() -> Result<ProgramFn<I, Self>, Box<dyn Error>> {
        Ok(|registers, _, _| {
            let incident: Vector = registers.pop();
            let normal: Vector = registers.pop();
            let env_ior: f32 = registers.pop();
            let ior: f32 = registers.pop();
            let value = crate::math::fresnel(ior, env_ior, normal.into(), incident.into());
            Vector(Vector4::new(value, value, value, value))
        })
    }
    fn blackbody() -> Result<ProgramFn<I, Self>, Box<dyn Error>> {
        Err("black-body functions cannot be used as vectors".into())
    }
}

impl Add for Vector {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        Vector(self.0.add_element_wise(rhs.0))
    }
}

impl Sub for Vector {
    type Output = Self;

    fn sub(self, rhs: Self) -> Self::Output {
        Vector(self.0.sub_element_wise(rhs.0))
    }
}

impl Mul for Vector {
    type Output = Self;

    fn mul(self, rhs: Self) -> Self::Output {
        Vector(self.0.mul_element_wise(rhs.0))
    }
}

impl Div for Vector {
    type Output = Self;

    fn div(self, rhs: Self) -> Self::Output {
        Vector(self.0.div_element_wise(rhs.0))
    }
}

impl Into<Vector2<f32>> for Vector {
    fn into(self) -> Vector2<f32> {
        self.0.truncate().truncate()
    }
}

impl Into<Vector3<f32>> for Vector {
    fn into(self) -> Vector3<f32> {
        self.0.truncate()
    }
}

impl From<Vector3<f32>> for Vector {
    fn from(vector: Vector3<f32>) -> Self {
        Vector(vector.extend(0.0))
    }
}

impl Into<Point2<f32>> for Vector {
    fn into(self) -> Point2<f32> {
        Point2::from_vec(self.0.truncate().truncate())
    }
}

impl From<Point2<f32>> for Vector {
    fn from(point: Point2<f32>) -> Self {
        Vector(point.to_vec().extend(0.0).extend(0.0))
    }
}

impl Into<Point3<f32>> for Vector {
    fn into(self) -> Point3<f32> {
        Point3::from_vec(self.0.truncate())
    }
}

impl Into<Quaternion<f32>> for Vector {
    fn into(self) -> Quaternion<f32> {
        Quaternion::new(self.0.x, self.0.y, self.0.z, self.0.w)
    }
}

#[derive(Debug, Copy, Clone, Ord, PartialOrd, Eq, PartialEq, Hash)]
#[repr(transparent)]
pub struct ExpressionId(usize);
