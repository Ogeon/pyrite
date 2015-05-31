use std::collections::HashMap;
use std::collections::hash_map::Entry::{Vacant, Occupied};
use std::any::Any;
use std::fmt;

pub use self::ConfigItem::{Primitive, List, Structure};

pub mod parser;

pub type PrimitiveType = parser::Value;

pub fn parse<C, F>(source: C, include: &mut F) -> Result<HashMap<String, ConfigItem>, String> where
    C: Iterator<Item=char>,
    F: FnMut(&String) -> Result<(String, Vec<String>), String>
{
    let mut items = HashMap::new();
    let instructions = parser::parse(source);

    for instruction in try!(instructions).into_iter() {
        try!(match instruction {
            parser::Action::Assign(path, parser::Value::Struct(template, instructions)) => {
                if instructions.len() == 0 {
                    match try!(deep_find(&items, &template).map(|v| v.map(|v| (*v).clone()))) {
                        Some(v) => deep_insert(&mut items, &path, v),
                        None => deep_insert(&mut items, &path, Structure(try!(get_typename(&template)), HashMap::new()))
                    }
                } else {
                    let (ty, mut fields) = try!(get_template(&items, &template), path.connect("."));

                    match evaluate(instructions, &mut fields, &items, include) {
                        Ok(()) => deep_insert(&mut items, &path, Structure(ty, fields)),
                        Err(e) => return Err(format!("{}: {}", path.connect("."), e))
                    }
                }
            },
            parser::Action::Assign(path, parser::Value::List(elements)) => {
                let elements = try!(evaluate_list(elements, &items, include), path.connect("."));

                deep_insert(&mut items, &path, List(elements))
            },
            parser::Action::Assign(path, primitive) => deep_insert(&mut items, &path, Primitive(primitive)),
            parser::Action::Include(source, path) => {
                let (code, source_path) = try!((*include)(&source));
                let path = match path {
                    Some(path) => path,
                    None => source_path
                };

                if path.len() == 0 {
                    return Err(format!("{} could not be turned into a path", source));
                } else {
                    let sub_structure = try!(parse(code.chars(), include), source);
                    deep_insert(&mut items, &path, Structure(Type::Untyped, sub_structure))
                }
            }
        })
    }

    Ok(items)
}

fn evaluate<F>(instructions: Vec<parser::Action>, scope: &mut HashMap<String, ConfigItem>, context: &HashMap<String, ConfigItem>, include: &mut F) -> Result<(), String> where
    F: FnMut(&String) -> Result<(String, Vec<String>), String>
{
    for instruction in instructions.into_iter() {
        try!(match instruction {
            parser::Action::Assign(path, parser::Value::Struct(template, instructions)) => {
                if instructions.len() == 0 {
                    match try!(deep_find(context, &template).map(|v| v.map(|v| (*v).clone()))) {
                        Some(v) => deep_insert(scope, &path, v),
                        None => deep_insert(scope, &path, Structure(try!(get_typename(&template)), HashMap::new()))
                    }
                } else {
                    let (ty, mut fields) = try!(get_template(context, &template), path.connect("."));

                    match evaluate(instructions, &mut fields, context, include) {
                        Ok(()) => deep_insert(scope, &path, Structure(ty, fields)),
                        Err(e) => return Err(format!("{}: {}", path.connect("."), e))
                    }
                }
            },
            parser::Action::Assign(path, parser::Value::List(elements)) => {
                let elements = try!(evaluate_list(elements, context, include), path.connect("."));

                deep_insert(scope, &path, List(elements))
            },
            parser::Action::Assign(path, primitive) => deep_insert(scope, &path, Primitive(primitive)),
            parser::Action::Include(source, path) => {
                let (code, source_path) = try!((*include)(&source));
                let path = match path {
                    Some(path) => path,
                    None => source_path
                };

                if path.len() == 0 {
                    return Err(format!("{} could not be turned into a path", source));
                } else {
                    let sub_structure = try!(parse(code.chars(), include), source);
                    deep_insert(scope, &path, Structure(Type::Untyped, sub_structure))
                }
            }
        })
    }

    Ok(())
}

fn evaluate_list<F>(elements: Vec<parser::Value>, context: &HashMap<String, ConfigItem>, include: &mut F) -> Result<Vec<ConfigItem>, String> where
    F: FnMut(&String) -> Result<(String, Vec<String>), String>
{
    let mut result = Vec::new();
    for (i, v) in elements.into_iter().enumerate() {
        match v {
            parser::Value::Struct(template, instructions) => {
                let (ty, mut fields) = try!(get_template(context, &template), format!("[{}]", i));

                match evaluate(instructions, &mut fields, context, include) {
                    Ok(()) => result.push(Structure(ty, fields)),
                    Err(e) => return Err(format!("[{}]: {}", i, e))
                }
            },
            parser::Value::List(elements) => result.push(List(try!(evaluate_list(elements, context, include), format!("[{}]", i)))),
            primitive => result.push(Primitive(primitive))
        }
    }

    Ok(result)
}

fn get_template(context: &HashMap<String, ConfigItem>, template: &Vec<String>) -> Result<(Type, HashMap<String, ConfigItem>), String> {
    match deep_find(context, template).map(|v| v.map(|v| (*v).clone())) {
        Ok(Some(Structure(template_type, fields))) => Ok((template_type, fields)),
        Ok(None) => Ok((try!(get_typename(template)), HashMap::new())),
        Ok(Some(_)) => Err("only a structure or a type can be used as a template".into()),
        Err(e) => Err(e)
    }
}

fn get_typename(template: &Vec<String>) -> Result<Type, String> {
    if template.len() == 0 {
        Ok(Type::Untyped)
    } else if template.len() == 1 {
        Ok(Type::Single(template.first().unwrap().clone()))
    } else if template.len() == 2 {
        Ok(Type::Grouped(template.first().unwrap().clone(), template.last().unwrap().clone()))
    } else {
        Err(format!("'{}' is not a valid type name", template.connect(".")))
    }
}

fn deep_insert(items: &mut HashMap<String, ConfigItem>, path: &[String], item: ConfigItem) -> Result<(), String> {
    if path.len() == 1 {
        items.insert(path.first().unwrap().clone(), item);
        Ok(())
    } else {
        let segment = path.first().unwrap();
        let rest = &path[1..];

        let parent = match items.entry(segment.clone()) {
            Vacant(entry) => entry.insert(Structure(Type::Untyped, HashMap::new())),
            Occupied(entry) => entry.into_mut()
        };

        match *parent {
            Structure(_, ref mut fields) => deep_insert(fields, rest, item).map_err(|e| format!("{}.{}", segment, e)),
            Primitive(ref v) => Err(format!("{}: expected a structure, but found primitive value '{:?}'", segment, v)),
            List(_) => Err(format!("{}: expected a structure, but found a list", segment))
        }
    }
}

fn deep_find<'a>(items: &'a HashMap<String, ConfigItem>, path: &Vec<String>) -> Result<Option<&'a ConfigItem>, String> {
    let mut items = items;
    let mut result = None;
    let end = path.len() - 1;

    for (i, segment) in path.iter().enumerate() {
        result = items.get(&segment.clone());
        if i < end {
            items = match result {
                Some(&Structure(_, ref fields)) => fields,
                Some(&Primitive(ref v)) => return Err(format!("{}: expected a structure, but found primitive value '{:?}'", path[0..i + 1].connect("."), v)),
                Some(&List(_)) => return Err(format!("{}: expected a structure, but found list", path[0..i + 1].connect("."))),
                None => return Ok(None)
            };
        }
    }

    Ok(result)
}



pub struct ConfigContext {
    groups: HashMap<String, HashMap<String, Box<Any + 'static>>>,
    types: HashMap<String, Box<Any + 'static>>
}

impl ConfigContext {
    pub fn new() -> ConfigContext {
        ConfigContext {
            groups: HashMap::new(),
            types: HashMap::new()
        }
    }

    pub fn insert_type<T: 'static, Ty: Into<String>>(&mut self, type_name: Ty, decoder: DecoderFn<T>) -> bool {
        let type_name = type_name.into();

        self.types.insert(type_name, Box::new(Decoder(decoder)) as Box<Any>).is_some()
    }

    pub fn insert_grouped_type<T: 'static, Gr: Into<String>, Ty: Into<String>>(&mut self, group_name: Gr, type_name: Ty, decoder: DecoderFn<T>) -> bool {
        let group_name = group_name.into();
        let type_name = type_name.into();
        
        let group = match self.groups.entry(group_name) {
            Vacant(entry) => entry.insert(HashMap::new()),
            Occupied(entry) => entry.into_mut()
        };
        
        group.insert(type_name, Box::new(Decoder(decoder)) as Box<Any>).is_some()
    }

    pub fn decode_structure_from_group<T: 'static, Gr: Into<String>>(&self, group_name: Gr, item: ConfigItem) -> Result<T, String> {
        let group_name = group_name.into();

        match item {
            Structure(Type::Grouped(item_group_name, type_name), fields) => if group_name == item_group_name {
                self.decode_structure(&Type::Grouped(group_name, type_name), fields)
            } else {
                Err(format!("expected a structure from group '{}', but found structure of type '{}.{}'", group_name, item_group_name, type_name))
            },
            value => Err(format!("expected a structure from group '{}', but found {}", group_name, value))
        }
    }

    pub fn decode_structure_from_groups<T: 'static, Gr: Into<String>>(&self, group_names: Vec<Gr>, item: ConfigItem) -> Result<T, String> {
        let group_names = group_names.into_iter().map(|n| n.into()).collect::<Vec<String>>();

        let name_collection = if group_names.len() == 1 {
            format!("'{}'", group_names.first().unwrap())
        } else if group_names.len() > 1 {
            let names = &group_names[..group_names.len() - 1];
            format!("'{}' or '{}'", names.connect("', '"), group_names.last().unwrap())
        } else {
            return Err("internal error: trying to decode structure from one of 0 groups".into());
        };

        match item {
            Structure(Type::Grouped(group_name, type_name), fields) => if group_names.contains(&group_name) {
                self.decode_structure(&Type::Grouped(group_name, type_name), fields)
            } else {
                Err(format!("expected a structure from group {:?}, but found structure of type '{}.{}'", group_names, group_name, type_name))
            },
            value => Err(format!("expected a structure from group {}, but found {}", name_collection, value))
        }
    }

    pub fn decode_structure_of_type<T: 'static>(&self, structure_type: &Type, item: ConfigItem) -> Result<T, String> {
        match item {
            Structure(ty, fields) => if &ty == structure_type {
                self.decode_structure(structure_type, fields.clone())
            } else {
                Err(format!("expected {}, but found {}", structure_type, ty))
            },
            value => Err(format!("expected {}, but found {}", structure_type, value))
        }
    }

    fn decode_structure<T: 'static>(&self, structure_type: &Type, fields: HashMap<String, ConfigItem>) -> Result<T, String> {
        match *structure_type {
            Type::Single(ref type_name) => {
                match self.types.get(type_name) {
                    Some(decoder) => match decoder.downcast_ref::<Decoder<T>>() {
                        Some(decoder) => decoder.decode(self, fields),
                        None => Err(format!("type cannot be decoded as '{}'", type_name))
                    },
                    None => Err(format!("unknown type '{}'", type_name))
                }
            },
            Type::Grouped(ref group_name, ref type_name) => {
                match self.groups.get(group_name).and_then(|group| group.get(type_name)) {
                    Some(decoder) => match decoder.downcast_ref::<Decoder<T>>() {
                        Some(decoder) => decoder.decode(self, fields),
                        None => Err(format!("type cannot be decoded as '{}.{}'", group_name, type_name))
                    },
                    None => Err(format!("unknown type '{}.{}'", group_name, type_name))
                }
            },
            _ => Err("internal error: contextual decoding of untyped structure".into())
        }
    }
}

pub type DecoderFn<T> = fn(&ConfigContext, HashMap<String, ConfigItem>) -> Result<T, String>;

struct Decoder<T>(DecoderFn<T>);

impl<T> Decoder<T>  {
    fn decode(&self, context: &ConfigContext, fields: HashMap<String, ConfigItem>) -> Result<T, String> {
        let &Decoder(decoder) = self;
        decoder(context, fields)
    }
}


#[derive(Clone, PartialEq, Eq)]
pub enum Type {
    Single(String),
    Grouped(String, String),
    Untyped
}

impl Type {
    pub fn single<Ty: Into<String>>(type_name: Ty) -> Type {
        Type::Single(type_name.into())
    }
}

impl fmt::Display for Type {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            Type::Single(ref type_name) => write!(f, "structure of type '{}'", type_name),
            Type::Grouped(ref group_name, ref type_name) => write!(f, "structure of type '{}.{}'", group_name, type_name),
            Type::Untyped => write!(f, "untyped structure")
        }
    }
}

#[derive(Clone)]
pub enum ConfigItem {
    Structure(Type, HashMap<String, ConfigItem>),
    List(Vec<ConfigItem>),
    Primitive(parser::Value)
}

impl fmt::Display for ConfigItem {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            Structure(ref type_name, _) => type_name.fmt(f),
            List(ref l) => write!(f, "list of length {}", l.len()),
            Primitive(ref v) => write!(f, "'{:?}'", v)
        }
    }
}

impl ConfigItem {
    pub fn into_list(self) -> Result<Vec<ConfigItem>, String> {
        match self {
            List(v) => Ok(v),
            v => Err(format!("expected a list, but found {}", v))
        }
    }
}

pub trait FromConfig: Sized {
    fn from_primitive(item: PrimitiveType) -> Result<Self, String> {
        Err(format!("unexpected '{:?}'", item))
    }

    fn from_structure(structure_type: Type, _fields: HashMap<String, ConfigItem>) -> Result<Self, String> {
        Err(format!("unexpected {}", structure_type))
    }

    fn from_list(elements: Vec<ConfigItem>) -> Result<Self, String> {
        Err(format!("unexpected list of length {}", elements.len()))
    }

    fn from_config(item: ConfigItem) -> Result<Self, String> {
        match item {
            Structure(ty, fields) => FromConfig::from_structure(ty, fields),
            Primitive(item) => FromConfig::from_primitive(item),
            List(elements) => FromConfig::from_list(elements)
        }
    }
}

impl FromConfig for f64 {
    fn from_primitive(item: PrimitiveType) -> Result<f64, String> {
        match item {
            parser::Value::Number(f) => Ok(f),
            _ => Err("expected a number".into())
        }
    }
}

impl FromConfig for f32 {
    fn from_primitive(item: PrimitiveType) -> Result<f32, String> {
        match item {
            parser::Value::Number(f) => Ok(f as f32),
            _ => Err("expected a number".into())
        }
    }
}

impl FromConfig for usize {
    fn from_primitive(item: PrimitiveType) -> Result<usize, String> {
        match item {
            parser::Value::Number(f) if f >= 0.0 => Ok(f as usize),
            parser::Value::Number(_) => Err("expected a positive integer".into()),
            _ => Err("expected a number".into())
        }
    }
}

impl FromConfig for u32 {
    fn from_primitive(item: PrimitiveType) -> Result<u32, String> {
        match item {
            parser::Value::Number(f) if f >= 0.0 => Ok(f as u32),
            parser::Value::Number(_) => Err("expected a positive integer".into()),
            _ => Err("expected a number".into())
        }
    }
}

impl FromConfig for String {
    fn from_primitive(item: PrimitiveType) -> Result<String, String> {
        match item {
            parser::Value::Str(s) => Ok(s),
            _ => Err("expected a string".into())
        }
    }
}

impl<T: FromConfig> FromConfig for Vec<T> {
    fn from_list(items: Vec<ConfigItem>) -> Result<Vec<T>, String> {
        let mut decoded = Vec::new();

        for (i, item) in items.into_iter().enumerate() {
            decoded.push(try!(FromConfig::from_config(item), format!("[{}]", i)))
        }

        Ok(decoded)
    }
}

impl<A: FromConfig, B: FromConfig> FromConfig for (A, B) {
    fn from_list(items: Vec<ConfigItem>) -> Result<(A, B), String> {
        if items.len() != 2 {
            Err(format!("expected a list of length 2, but found a list of length {}", items.len()))
        } else {
            let mut items = items.into_iter();
            let a = try!(FromConfig::from_config(items.next().unwrap()), "[0]");
            let b = try!(FromConfig::from_config(items.next().unwrap()), "[1]");
            Ok((a, b))
        }
    }
}