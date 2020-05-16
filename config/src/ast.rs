#[derive(PartialEq, Debug)]
pub enum Statement {
    Include(String, Option<Path>),
    Assign(Path, Value),
}

#[derive(PartialEq, Debug)]
pub enum PathType {
    Global,
    Local,
}

#[derive(PartialEq, Debug)]
pub struct Path {
    pub path_type: PathType,
    pub path: Vec<String>,
}

#[derive(PartialEq, Debug)]
pub enum Value {
    Object(Object),
    Number(Number),
    String(String),
    List(Vec<Value>),
}

#[derive(PartialEq, Debug)]
pub enum Object {
    New(Vec<(Path, Value)>),
    Extension(Path, Option<ExtensionChanges>),
}

#[derive(PartialEq, Debug)]
pub enum ExtensionChanges {
    BlockStyle(Vec<(Path, Value)>),
    FunctionStyle(Vec<Value>),
}

///A float or an integer.
#[derive(PartialEq, Debug, Clone, Copy)]
pub enum Number {
    Integer(i32),
    Float(f32),
}

impl Number {
    pub fn as_float(self) -> f32 {
        match self {
            Number::Integer(i) => i as f32,
            Number::Float(f) => f,
        }
    }
}
