use std::{
    collections::{BTreeMap, HashMap},
    convert::TryFrom,
    error::Error,
    path::{Path, PathBuf},
};

use rlua::Lua;
use rlua_serde;

use cgmath::{Matrix4, Point3, Quaternion, SquareMatrix, Vector3};
use serde::{Deserialize, Deserializer};

pub fn load_project<P: AsRef<Path>>(path: P) -> Result<Project, Box<dyn Error>> {
    let project_dir = path
        .as_ref()
        .parent()
        .expect("could not get the project path parent directory");

    let lua = Lua::new();

    lua.context(|context| {
        context
            .load(&format!(
                r#"package.path = "{};" .. package.path"#,
                project_dir
                    .join("?.lua")
                    .to_str()
                    .expect("could not convert project path to UTF8")
            ))
            .set_name("<pyrite>")?
            .exec()?;

        context
            .load(include_str!("lib.lua"))
            .set_name("<pyrite>/lib.lua")?
            .exec()?;

        let project_file = std::fs::read_to_string(&path)?;
        let project = context
            .load(&project_file)
            .set_name(
                path.as_ref()
                    .file_name()
                    .and_then(std::ffi::OsStr::to_str)
                    .unwrap_or_else(|| "<project file>"),
            )?
            .eval()?;

        let project = rlua_serde::from_value(project)?;
        Ok(project)
    })
}

#[derive(Deserialize)]
pub struct Project {
    pub image: Image,
    pub camera: Camera,
    pub renderer: Renderer,
    pub world: World,
}

#[derive(Deserialize)]
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub file: Option<String>,
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Camera {
    Perspective {
        transform: Transform,
        fov: Expression,
        focus_distance: Option<Expression>,
        aperture: Option<Expression>,
    },
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Renderer {
    Simple {
        #[serde(flatten)]
        shared: RendererShared,
    },
    Bidirectional {
        #[serde(flatten)]
        shared: RendererShared,
        light_bounces: Option<Expression>,
    },
    PhotonMapping {
        #[serde(flatten)]
        shared: RendererShared,
        radius: Option<Expression>,
        photons: Option<Expression>,
        photon_bounces: Option<Expression>,
        photon_passes: Option<Expression>,
    },
}

#[derive(Deserialize)]
pub struct RendererShared {
    pub pixel_samples: Expression,
    pub threads: Option<Expression>,
    pub bounces: Option<Expression>,
    pub light_samples: Option<Expression>,
    pub tile_size: Option<Expression>,
    pub spectrum_samples: Option<Expression>,
    pub spectrum_resolution: Option<Expression>,
}

#[derive(Deserialize)]
pub struct World {
    pub sky: Option<Expression>,
    pub objects: Vec<WorldObject>,
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WorldObject {
    Sphere {
        position: Expression,
        radius: Expression,
        material: Material,
    },
    Plane {
        origin: Expression,
        normal: Expression,
        binormal: Option<Expression>,
        texture_scale: Option<Expression>,
        material: Material,
    },
    RayMarched {
        shape: Estimator,
        bounds: BoundingVolume,
        material: Material,
    },
    Mesh {
        file: String,
        materials: HashMap<String, Material>,
        scale: Option<Expression>,
        transform: Option<Transform>,
    },
    DirectionalLight {
        direction: Expression,
        width: Expression,
        color: Expression,
    },
    PointLight {
        position: Expression,
        color: Expression,
    },
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BoundingVolume {
    Box {
        min: Expression,
        max: Expression,
    },
    Sphere {
        position: Expression,
        radius: Expression,
    },
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Estimator {
    Mandelbulb {
        iterations: Expression,
        threshold: Expression,
        power: Expression,
        constant: Option<Expression>,
    },
    QuaternionJulia {
        iterations: Expression,
        threshold: Expression,
        constant: Expression,
        slice_plane: Expression,
        variant: JuliaType,
    },
}

#[derive(Deserialize)]
pub struct JuliaType {
    pub name: String,
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Material {
    Diffuse {
        color: Expression,
    },
    Emission {
        color: Expression,
    },
    Mirror {
        color: Expression,
    },
    Refractive {
        color: Expression,
        ior: Expression,
        dispersion: Option<Expression>,
        env_ior: Option<Expression>,
        env_dispersion: Option<Expression>,
    },
    Mix {
        factor: Expression,
        a: Box<Material>,
        b: Box<Material>,
    },
    FresnelMix {
        ior: Expression,
        dispersion: Option<Expression>,
        env_ior: Option<Expression>,
        env_dispersion: Option<Expression>,
        reflect: Box<Material>,
        refract: Box<Material>,
    },
}

#[derive(Clone)]
pub enum Expression {
    Number(f64),
    Boolean(bool),
    Complex(ComplexExpression),
}

impl Expression {
    pub fn parse<T: FromExpression>(
        self,
        make_path: &impl Fn(&str) -> PathBuf,
    ) -> Result<T, Box<dyn Error>> {
        T::from_expression(self, make_path)
    }
}

impl<'de> Deserialize<'de> for Expression {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = serde_value::Value::deserialize(deserializer)?;

        match value {
            serde_value::Value::Bool(value) => Ok(Expression::Boolean(value)),
            serde_value::Value::U8(value) => Ok(Expression::Number(value as f64)),
            serde_value::Value::U16(value) => Ok(Expression::Number(value as f64)),
            serde_value::Value::U32(value) => Ok(Expression::Number(value as f64)),
            serde_value::Value::U64(value) => Ok(Expression::Number(value as f64)),
            serde_value::Value::I8(value) => Ok(Expression::Number(value as f64)),
            serde_value::Value::I16(value) => Ok(Expression::Number(value as f64)),
            serde_value::Value::I32(value) => Ok(Expression::Number(value as f64)),
            serde_value::Value::I64(value) => Ok(Expression::Number(value as f64)),
            serde_value::Value::F32(value) => Ok(Expression::Number(value as f64)),
            serde_value::Value::F64(value) => Ok(Expression::Number(value as f64)),
            value => Ok(Expression::Complex(ComplexExpression::deserialize(
                serde_value::ValueDeserializer::<D::Error>::new(value),
            )?)),
        }
    }
}

#[derive(Deserialize, Clone)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ComplexExpression {
    Vector {
        x: rlua::Number,
        y: rlua::Number,
        z: rlua::Number,
        w: rlua::Number,
    },
    Fresnel {
        ior: Box<Expression>,
        env_ior: Option<Box<Expression>>,
    },
    LightSource {
        name: String,
    },
    Spectrum {
        points: List<List<rlua::Number>>,
    },
    Rgb {
        red: Box<Expression>,
        green: Box<Expression>,
        blue: Box<Expression>,
    },
    Texture {
        path: String,
    },
    Add {
        a: Box<Expression>,
        b: Box<Expression>,
    },
    Sub {
        a: Box<Expression>,
        b: Box<Expression>,
    },
    Mul {
        a: Box<Expression>,
        b: Box<Expression>,
    },
    Div {
        a: Box<Expression>,
        b: Box<Expression>,
    },
    Mix {
        a: Box<Expression>,
        b: Box<Expression>,
        factor: Box<Expression>,
    },
}

#[derive(Clone)]
pub struct List<T>(BTreeMap<usize, T>);

impl<'de, T: Deserialize<'de>> Deserialize<'de> for List<T> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Ok(List(BTreeMap::deserialize(deserializer)?))
    }
}

impl<T> TryFrom<List<T>> for (T, T) {
    type Error = Box<dyn Error>;

    fn try_from(mut value: List<T>) -> Result<Self, Self::Error> {
        let result = if let (Some(a), Some(b)) = (value.0.remove(&1), value.0.remove(&2)) {
            if value.0.is_empty() {
                Some((a, b))
            } else {
                None
            }
        } else {
            None
        };

        if let Some(result) = result {
            Ok(result)
        } else {
            Err("expected exactly two values".into())
        }
    }
}

impl<T> TryFrom<List<T>> for Vec<T> {
    type Error = Box<dyn Error>;

    fn try_from(value: List<T>) -> Result<Self, Self::Error> {
        let mut result = Vec::with_capacity(value.0.len());

        for (index, (key, value)) in value.0.into_iter().enumerate() {
            if key != index + 1 {
                return Err("the list is not sequential".into());
            }

            result.push(value);
        }

        Ok(result)
    }
}

pub trait FromExpression: Sized {
    fn from_expression(
        expression: Expression,
        make_path: &impl Fn(&str) -> PathBuf,
    ) -> Result<Self, Box<dyn Error>>;

    fn from_expression_or(
        expression: Option<Expression>,
        make_path: &impl Fn(&str) -> PathBuf,
        default: Self,
    ) -> Result<Self, Box<dyn Error>> {
        expression
            .map(|e| Self::from_expression(e, make_path))
            .unwrap_or(Ok(default))
    }

    fn from_expression_or_else(
        expression: Option<Expression>,
        make_path: &impl Fn(&str) -> PathBuf,
        get_default: impl FnOnce() -> Self,
    ) -> Result<Self, Box<dyn Error>> {
        expression
            .map(|e| Self::from_expression(e, make_path))
            .unwrap_or_else(|| Ok(get_default()))
    }
}

impl FromExpression for f64 {
    fn from_expression(
        expression: Expression,
        _make_path: &impl Fn(&str) -> PathBuf,
    ) -> Result<Self, Box<dyn Error>> {
        match expression {
            Expression::Number(number) => Ok(number),
            _ => Err("expected a number".into()),
        }
    }
}
impl FromExpression for f32 {
    fn from_expression(
        expression: Expression,
        make_path: &impl Fn(&str) -> PathBuf,
    ) -> Result<Self, Box<dyn Error>> {
        let number: f64 = expression.parse(make_path)?;
        Ok(number as f32)
    }
}

impl FromExpression for u16 {
    fn from_expression(
        expression: Expression,
        make_path: &impl Fn(&str) -> PathBuf,
    ) -> Result<Self, Box<dyn Error>> {
        let number: f64 = expression.parse(make_path)?;
        Ok(number as u16)
    }
}

impl FromExpression for u32 {
    fn from_expression(
        expression: Expression,
        make_path: &impl Fn(&str) -> PathBuf,
    ) -> Result<Self, Box<dyn Error>> {
        let number: f64 = expression.parse(make_path)?;
        Ok(number as u32)
    }
}

impl FromExpression for usize {
    fn from_expression(
        expression: Expression,
        make_path: &impl Fn(&str) -> PathBuf,
    ) -> Result<Self, Box<dyn Error>> {
        let number: f64 = expression.parse(make_path)?;
        Ok(number as usize)
    }
}

impl FromExpression for bool {
    fn from_expression(
        expression: Expression,
        _make_path: &impl Fn(&str) -> PathBuf,
    ) -> Result<Self, Box<dyn Error>> {
        match expression {
            Expression::Boolean(boolean) => Ok(boolean),
            _ => Err("expected a Boolean value".into()),
        }
    }
}

impl FromExpression for Vector3<f32> {
    fn from_expression(
        expression: Expression,
        _make_path: &impl Fn(&str) -> PathBuf,
    ) -> Result<Self, Box<dyn Error>> {
        match expression {
            Expression::Complex(ComplexExpression::Vector { x, y, z, .. }) => {
                Ok(Vector3::new(x as f32, y as f32, z as f32))
            }
            _ => Err("expected a vector".into()),
        }
    }
}

impl FromExpression for Point3<f32> {
    fn from_expression(
        expression: Expression,
        _make_path: &impl Fn(&str) -> PathBuf,
    ) -> Result<Self, Box<dyn Error>> {
        match expression {
            Expression::Complex(ComplexExpression::Vector { x, y, z, .. }) => {
                Ok(Point3::new(x as f32, y as f32, z as f32))
            }
            _ => Err("expected a vector".into()),
        }
    }
}

impl FromExpression for Quaternion<f32> {
    fn from_expression(
        expression: Expression,
        _make_path: &impl Fn(&str) -> PathBuf,
    ) -> Result<Self, Box<dyn Error>> {
        match expression {
            Expression::Complex(ComplexExpression::Vector { x, y, z, w }) => {
                Ok(Quaternion::new(x as f32, y as f32, z as f32, w as f32))
            }
            _ => Err("expected a vector".into()),
        }
    }
}

pub trait FromComplexExpression: Sized {
    fn from_complex_expression(
        expression: ComplexExpression,
        make_path: &impl Fn(&str) -> PathBuf,
    ) -> Result<Self, Box<dyn Error>>;
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Transform {
    LookAt {
        from: Expression,
        to: Expression,
        up: Option<Expression>,
    },
}

impl Transform {
    pub fn into_matrix(
        self,
        make_path: &impl Fn(&str) -> PathBuf,
    ) -> Result<Matrix4<f32>, Box<dyn Error>> {
        match self {
            crate::project::Transform::LookAt { from, to, up } => Matrix4::look_at(
                from.parse(make_path)?,
                to.parse(make_path)?,
                Vector3::from_expression_or_else(up, make_path, || Vector3::new(0.0, 1.0, 0.0))?,
            )
            .invert()
            .ok_or("could not invert view matrix".into()),
        }
    }
}
