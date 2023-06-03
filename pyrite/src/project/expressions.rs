use std::{
    error::Error,
    ops::{Add, Div, Mul, Sub},
};

use cgmath::{
    ElementWise, EuclideanSpace, Point2, Point3, Quaternion, Vector2, Vector3, Vector4, VectorSpace,
};
use typed_nodes::Key;

use crate::program::{FromValue, ProgramOutputType};

use super::{
    eval_context::{EvalContext, Evaluate},
    spectra::SpectrumId,
    textures::{ColorTextureId, MonoTextureId},
    Nodes,
};

pub(crate) fn insert_sub(nodes: &mut Nodes, lhs: Expression, rhs: Expression) -> Expression {
    if let (Expression::Number(lhs), Expression::Number(rhs)) = (lhs, rhs) {
        return Expression::Number(lhs - rhs);
    }

    let id = nodes.insert(ComplexExpression::Binary {
        operator: BinaryOperator::Sub,
        lhs,
        rhs,
    });

    Expression::Complex(id)
}

pub(crate) fn insert_mul(nodes: &mut Nodes, lhs: Expression, rhs: Expression) -> Expression {
    if let (Expression::Number(lhs), Expression::Number(rhs)) = (lhs, rhs) {
        return Expression::Number(lhs * rhs);
    }

    let id = nodes.insert(ComplexExpression::Binary {
        operator: BinaryOperator::Mul,
        lhs,
        rhs,
    });

    Expression::Complex(id)
}

pub(crate) fn insert_clamp(
    nodes: &mut Nodes,
    value: Expression,
    min: Expression,
    max: Expression,
) -> Expression {
    if let (Expression::Number(value), Expression::Number(min), Expression::Number(max)) =
        (value, min, max)
    {
        return Expression::Number(value.min(max).max(min));
    }

    let id = nodes.insert(ComplexExpression::Clamp { value, min, max });

    Expression::Complex(id)
}

#[derive(Copy, Clone, typed_nodes::FromLua)]
pub enum Expression {
    #[typed_nodes(untagged(number, integer))]
    Number(f64),
    #[typed_nodes(untagged(table))]
    Complex(Key<ComplexExpression>),
}

impl<T: ExpressionValue> Evaluate<T> for Expression {
    fn evaluate<'a>(&self, context: EvalContext<'a>) -> Result<T, Box<dyn Error>> {
        match *self {
            Expression::Number(number) => T::from_number(number),
            Expression::Complex(id) => context
                .nodes
                .get(id)
                .expect("missing expression")
                .evaluate(context),
        }
    }
}

impl Evaluate<Point3<f32>> for Expression {
    fn evaluate<'a>(&self, context: EvalContext<'a>) -> Result<Point3<f32>, Box<dyn Error>> {
        let vector = match *self {
            Expression::Number(number) => <Vector as ExpressionValue>::from_number(number),
            Expression::Complex(id) => context
                .nodes
                .get(id)
                .expect("missing expression")
                .evaluate(context),
        };

        Ok(vector?.into())
    }
}

impl Evaluate<Vector3<f32>> for Expression {
    fn evaluate<'a>(&self, context: EvalContext<'a>) -> Result<Vector3<f32>, Box<dyn Error>> {
        let vector = match *self {
            Expression::Number(number) => <Vector as ExpressionValue>::from_number(number),
            Expression::Complex(id) => context
                .nodes
                .get(id)
                .expect("missing expression")
                .evaluate(context),
        };

        Ok(vector?.into())
    }
}

impl Evaluate<Vector2<f32>> for Expression {
    fn evaluate<'a>(&self, context: EvalContext<'a>) -> Result<Vector2<f32>, Box<dyn Error>> {
        let vector = match *self {
            Expression::Number(number) => <Vector as ExpressionValue>::from_number(number),
            Expression::Complex(id) => context
                .nodes
                .get(id)
                .expect("missing expression")
                .evaluate(context),
        };

        Ok(vector?.into())
    }
}

impl Evaluate<Quaternion<f32>> for Expression {
    fn evaluate<'a>(&self, context: EvalContext<'a>) -> Result<Quaternion<f32>, Box<dyn Error>> {
        let vector = match *self {
            Expression::Number(number) => <Vector as ExpressionValue>::from_number(number),
            Expression::Complex(id) => context
                .nodes
                .get(id)
                .expect("missing expression")
                .evaluate(context),
        };

        Ok(vector?.into())
    }
}

impl From<f64> for Expression {
    fn from(number: f64) -> Self {
        Expression::Number(number)
    }
}

#[derive(typed_nodes::FromLua)]
#[typed_nodes(is_node)]
pub enum ComplexExpression {
    Vector {
        #[typed_nodes(recursive)]
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
    Clamp {
        value: Expression,
        min: Expression,
        max: Expression,
    },
    Fresnel {
        ior: Expression,
        env_ior: Expression,
    },
    Blackbody {
        temperature: Expression,
    },
    Spectrum {
        #[typed_nodes(flatten)]
        points: SpectrumId,
    },
    ColorTexture {
        #[typed_nodes(flatten)]
        texture: ColorTextureId,
    },
    MonoTexture {
        #[typed_nodes(flatten)]
        texture: MonoTextureId,
    },
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
            ComplexExpression::Clamp { value, min, max } => {
                let value = value.evaluate(context)?;
                let min = min.evaluate(context)?;
                let max = max.evaluate(context)?;
                T::clamp(value, min, max)
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
            ComplexExpression::ColorTexture { .. } | ComplexExpression::MonoTexture { .. } => {
                Err("cannot evaluate textures as constants".into())
            }
        }
    }
}

#[derive(Copy, Clone, typed_nodes::FromLua)]
pub enum BinaryOperator {
    Add,
    Sub,
    Mul,
    Div,
}

pub trait ExpressionValue:
    Sized + Add<Output = Self> + Sub<Output = Self> + Mul<Output = Self> + Div<Output = Self>
{
    fn from_number(number: f64) -> Result<Self, Box<dyn Error>>;
    fn from_vector(x: f32, y: f32, z: f32, w: f32) -> Result<Self, Box<dyn Error>>;
    fn from_rgb(red: f32, green: f32, blue: f32) -> Result<Self, Box<dyn Error>>;
    fn mix(lhs: Self, rhs: Self, amount: f32) -> Result<Self, Box<dyn Error>>;
    fn clamp(value: Self, min: Self, max: Self) -> Result<Self, Box<dyn Error>>;
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

    fn clamp(value: Self, min: Self, max: Self) -> Result<Self, Box<dyn Error>> {
        Ok(value.min(max).max(min))
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

    fn clamp(value: Self, min: Self, max: Self) -> Result<Self, Box<dyn Error>> {
        Ok(value.min(max).max(min))
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

    fn clamp(_value: Self, _min: Self, _max: Self) -> Result<Self, Box<dyn Error>> {
        Err("vectors cannot be clamped".into())
    }
}

impl FromValue for Vector {
    fn from_number() -> Result<fn(f32) -> Self, Box<dyn Error>> {
        Ok(|number| Vector(Vector4::new(number, number, number, number)))
    }

    fn from_value() -> ProgramOutputType<Self> {
        ProgramOutputType::Vector(|vector| vector)
    }
}

impl Default for Vector {
    fn default() -> Self {
        Vector(Vector4::new(0.0, 0.0, 0.0, 0.0))
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

impl From<Vector4<f32>> for Vector {
    fn from(vector: Vector4<f32>) -> Self {
        Vector(vector)
    }
}

impl Into<Vector4<f32>> for Vector {
    fn into(self) -> Vector4<f32> {
        self.0
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
