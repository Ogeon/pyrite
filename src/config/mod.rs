use std::collections::HashMap;
use std::any::{Any, AnyRefExt};
use std::str::StrAllocating;
use std::fmt;

pub use self::parser::String;
pub use self::parser::Number;

mod parser;

pub type PrimitiveType = parser::Value;
type IncludeFn<'a> = |&String|:'a -> Result<(String, Vec<String>), String>;

pub fn parse<C: Iterator<char>>(source: C, include: &mut IncludeFn) -> Result<HashMap<String, ConfigItem>, String> {
    let mut items = HashMap::new();
    let instructions = parser::parse(source);

    for instruction in try!(instructions).move_iter() {
        try!(match instruction {
            parser::Assign(path, parser::Struct(template, instructions)) => {
                if instructions.len() == 0 {
                    match try!(deep_find(&items, &template).map(|v| v.map(|v| (*v).clone()))) {
                        Some(v) => deep_insert(&mut items, path.as_slice(), v),
                        None => deep_insert(&mut items, path.as_slice(), Structure(try!(get_typename(&template)), HashMap::new()))
                    }
                } else {
                    let (ty, mut fields) = try!(get_template(&items, &template), path.as_slice().connect("."));

                    match evaluate(instructions, &mut fields, &items, include) {
                        Ok(()) => deep_insert(&mut items, path.as_slice(), Structure(ty, fields)),
                        Err(e) => return Err(format!("{}: {}", path.as_slice().connect("."), e))
                    }
                }
            },
            parser::Assign(path, parser::List(elements)) => {
                let elements = try!(evaluate_list(elements, &items, include), path.as_slice().connect("."));

                deep_insert(&mut items, path.as_slice(), List(elements))
            },
            parser::Assign(path, primitive) => deep_insert(&mut items, path.as_slice(), Primitive(primitive)),
            parser::Include(source, path) => {
                let (code, source_path) = try!((*include)(&source));
                let path = match path {
                    Some(path) => path,
                    None => source_path
                };

                if path.len() == 0 {
                    return Err(format!("{} could not be turned into a path", source));
                } else {
                    let sub_structure = try!(parse(code.as_slice().chars(), include), source);
                    deep_insert(&mut items, path.as_slice(), Structure(Untyped, sub_structure))
                }
            }
        })
    }

    Ok(items)
}

fn evaluate(instructions: Vec<parser::Action>, scope: &mut HashMap<String, ConfigItem>, context: &HashMap<String, ConfigItem>, include: &mut IncludeFn) -> Result<(), String> {
    for instruction in instructions.move_iter() {
        try!(match instruction {
            parser::Assign(path, parser::Struct(template, instructions)) => {
                if instructions.len() == 0 {
                    match try!(deep_find(context, &template).map(|v| v.map(|v| (*v).clone()))) {
                        Some(v) => deep_insert(scope, path.as_slice(), v),
                        None => deep_insert(scope, path.as_slice(), Structure(try!(get_typename(&template)), HashMap::new()))
                    }
                } else {
                    let (ty, mut fields) = try!(get_template(context, &template), path.as_slice().connect("."));

                    match evaluate(instructions, &mut fields, context, include) {
                        Ok(()) => deep_insert(scope, path.as_slice(), Structure(ty, fields)),
                        Err(e) => return Err(format!("{}: {}", path.as_slice().connect("."), e))
                    }
                }
            },
            parser::Assign(path, parser::List(elements)) => {
                let elements = try!(evaluate_list(elements, context, include), path.as_slice().connect("."));

                deep_insert(scope, path.as_slice(), List(elements))
            },
            parser::Assign(path, primitive) => deep_insert(scope, path.as_slice(), Primitive(primitive)),
            parser::Include(source, path) => {
                let (code, source_path) = try!((*include)(&source));
                let path = match path {
                    Some(path) => path,
                    None => source_path
                };

                if path.len() == 0 {
                    return Err(format!("{} could not be turned into a path", source));
                } else {
                    let sub_structure = try!(parse(code.as_slice().chars(), include), source);
                    deep_insert(scope, path.as_slice(), Structure(Untyped, sub_structure))
                }
            }
        })
    }

    Ok(())
}

fn evaluate_list(elements: Vec<parser::Value>, context: &HashMap<String, ConfigItem>, include: &mut IncludeFn) -> Result<Vec<ConfigItem>, String> {
    let mut result = Vec::new();
    for (i, v) in elements.move_iter().enumerate() {
        match v {
            parser::Struct(template, instructions) => {
                let (ty, mut fields) = try!(get_template(context, &template), format!("[{}]", i));

                match evaluate(instructions, &mut fields, context, include) {
                    Ok(()) => result.push(Structure(ty, fields)),
                    Err(e) => return Err(format!("[{}]: {}", i, e))
                }
            },
            parser::List(elements) => result.push(List(try!(evaluate_list(elements, context, include), format!("[{}]", i)))),
            primitive => result.push(Primitive(primitive))
        }
    }

    Ok(result)
}

fn get_template(context: &HashMap<String, ConfigItem>, template: &Vec<String>) -> Result<(Type, HashMap<String, ConfigItem>), String> {
    match deep_find(context, template).map(|v| v.map(|v| (*v).clone())) {
        Ok(Some(Structure(template_type, fields))) => Ok((template_type, fields)),
        Ok(None) => Ok((try!(get_typename(template)), HashMap::new())),
        Ok(Some(_)) => Err(String::from_str("only a structure or a type can be used as a template")),
        Err(e) => Err(e)
    }
}

fn get_typename(template: &Vec<String>) -> Result<Type, String> {
    match template.as_slice() {
        [ref type_name] => Ok(Single(type_name.clone())),
        [ref group_name, ref type_name] => Ok(Grouped(group_name.clone(), type_name.clone())),
        [] => Ok(Untyped),
        _ => Err(format!("'{}' is not a valid type name", template.as_slice().connect(".")))
    }
}

fn deep_insert(items: &mut HashMap<String, ConfigItem>, path: &[String], item: ConfigItem) -> Result<(), String> {
    match path {
        [ref segment] => {
            items.insert(segment.clone(), item);
            Ok(())
        },
        [ref segment, ..rest] => {
            match items.find_or_insert_with(segment.clone(), |_| Structure(Untyped, HashMap::new()) ) {
                &Structure(_, ref mut fields) => deep_insert(fields, rest, item).map_err(|e| format!("{}.{}", segment, e)),
                &Primitive(ref v) => Err(format!("{}: expected a structure, but found primitive value '{}'", segment, v)),
                &List(_) => Err(format!("{}: expected a structure, but found a list", segment))
            }
        },
        [] => unreachable!()
    }
}

fn deep_find<'a>(items: &'a HashMap<String, ConfigItem>, path: &Vec<String>) -> Result<Option<&'a ConfigItem>, String> {
    let mut items = items;
    let mut result = None;
    let end = path.len() - 1;

    for (i, segment) in path.iter().enumerate() {
        result = items.find(&segment.clone());
        if i < end {
            items = match result {
                Some(&Structure(_, ref fields)) => fields,
                Some(&Primitive(ref v)) => return Err(format!("{}: expected a structure, but found primitive value '{}'", path.slice(0, i + 1).connect("."), v)),
                Some(&List(_)) => return Err(format!("{}: expected a structure, but found list", path.slice(0, i + 1).connect("."))),
                None => return Ok(None)
            };
        }
    }

    Ok(result)
}



pub struct ConfigContext {
    groups: HashMap<String, HashMap<String, Box<Any>>>,
    types: HashMap<String, Box<Any>>
}

impl ConfigContext {
    pub fn new() -> ConfigContext {
        ConfigContext {
            groups: HashMap::new(),
            types: HashMap::new()
        }
    }

    pub fn insert_type<T: 'static, Ty: StrAllocating>(&mut self, type_name: Ty, decoder: DecoderFn<T>) -> bool {
        let type_name = type_name.into_string();

        self.types.insert(type_name, box Decoder(decoder) as Box<Any>)
    }

    pub fn insert_grouped_type<T: 'static, Gr: StrAllocating, Ty: StrAllocating>(&mut self, group_name: Gr, type_name: Ty, decoder: DecoderFn<T>) -> bool {
        let group_name = group_name.into_string();
        let type_name = type_name.into_string();

        self.groups.find_or_insert_with(group_name, |_| HashMap::new()).insert(type_name, box Decoder(decoder) as Box<Any>)
    }

    pub fn decode_structure_from_group<T: 'static, Gr: StrAllocating>(&self, group_name: Gr, item: ConfigItem) -> Result<T, String> {
        let group_name = group_name.into_string();

        match item {
            Structure(Grouped(item_group_name, type_name), fields) => if group_name == item_group_name {
                self.decode_structure(&Grouped(group_name, type_name), fields)
            } else {
                Err(format!("expected a structure from group '{}', but found structure of type '{}.{}'", group_name, item_group_name, type_name))
            },
            value => Err(format!("expected a structure from group '{}', but found {}", group_name, value))
        }
    }

    pub fn decode_structure_from_groups<T: 'static, Gr: StrAllocating>(&self, group_names: Vec<Gr>, item: ConfigItem) -> Result<T, String> {
        let group_names = group_names.move_iter().map(|n| n.into_string()).collect::<Vec<String>>();

        let name_collection = match group_names.as_slice() {
            [ref name] => format!("'{}'", name),
            [..names, ref last] => format!("'{}' or '{}'", names.connect("', '"), last),
            [] => return Err(String::from_str("internal error: trying to decode structure from one of 0 groups"))
        };

        match item {
            Structure(Grouped(group_name, type_name), fields) => if group_names.contains(&group_name) {
                self.decode_structure(&Grouped(group_name, type_name), fields)
            } else {
                Err(format!("expected a structure from group {}, but found structure of type '{}.{}'", group_names, group_name, type_name))
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
            Single(ref type_name) => {
                match self.types.find(type_name) {
                    Some(decoder) => match decoder.downcast_ref::<Decoder<T>>() {
                        Some(decoder) => decoder.decode(self, fields),
                        None => Err(format!("type cannot be decoded as '{}'", type_name))
                    },
                    None => Err(format!("unknown type '{}'", type_name))
                }
            },
            Grouped(ref group_name, ref type_name) => {
                match self.groups.find(group_name).and_then(|group| group.find(type_name)) {
                    Some(decoder) => match decoder.downcast_ref::<Decoder<T>>() {
                        Some(decoder) => decoder.decode(self, fields),
                        None => Err(format!("type cannot be decoded as '{}.{}'", group_name, type_name))
                    },
                    None => Err(format!("unknown type '{}.{}'", group_name, type_name))
                }
            },
            _ => Err(String::from_str("internal error: contextual decoding of untyped structure"))
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


#[deriving(Clone, PartialEq, Eq)]
pub enum Type {
    Single(String),
    Grouped(String, String),
    Untyped
}

impl Type {
    pub fn single<Ty: StrAllocating>(type_name: Ty) -> Type {
        Single(type_name.into_string())
    }
}

impl fmt::Show for Type {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            Single(ref type_name) => write!(f, "structure of type '{}'", type_name),
            Grouped(ref group_name, ref type_name) => write!(f, "structure of type '{}.{}'", group_name, type_name),
            Untyped => write!(f, "untyped structure")
        }
    }
}

#[deriving(Clone)]
pub enum ConfigItem {
    Structure(Type, HashMap<String, ConfigItem>),
    List(Vec<ConfigItem>),
    Primitive(parser::Value)
}

impl fmt::Show for ConfigItem {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            Structure(ref type_name, _) => type_name.fmt(f),
            List(ref l) => write!(f, "list of length {}", l.len()),
            Primitive(ref v) => write!(f, "'{}'", v)
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

pub trait FromConfig {
    fn from_primitive(item: PrimitiveType) -> Result<Self, String> {
        Err(format!("unexpected '{}'", item))
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
            parser::Number(f) => Ok(f),
            _ => Err(String::from_str("expected a number"))
        }
    }
}

impl FromConfig for f32 {
    fn from_primitive(item: PrimitiveType) -> Result<f32, String> {
        match item {
            parser::Number(f) => Ok(f as f32),
            _ => Err(String::from_str("expected a number"))
        }
    }
}

impl FromConfig for uint {
    fn from_primitive(item: PrimitiveType) -> Result<uint, String> {
        match item {
            parser::Number(f) if f >= 0.0 => Ok(f as uint),
            parser::Number(_) => Err(String::from_str("expected a positive integer")),
            _ => Err(String::from_str("expected a number"))
        }
    }
}

impl FromConfig for String {
    fn from_primitive(item: PrimitiveType) -> Result<String, String> {
        match item {
            parser::String(s) => Ok(s),
            _ => Err(String::from_str("expected a string"))
        }
    }
}

impl<T: FromConfig> FromConfig for Vec<T> {
    fn from_list(items: Vec<ConfigItem>) -> Result<Vec<T>, String> {
        let mut decoded = Vec::new();

        for (i, item) in items.move_iter().enumerate() {
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
            let mut items = items.move_iter();
            let a = try!(FromConfig::from_config(items.next().unwrap()), "[0]");
            let b = try!(FromConfig::from_config(items.next().unwrap()), "[1]");
            Ok((a, b))
        }
    }
}