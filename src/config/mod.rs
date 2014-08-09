use std::collections::HashMap;
use std::any::{Any, AnyRefExt};
use std::str::StrAllocating;

pub use self::parser::String;
pub use self::parser::Number;

mod parser;

pub type PrimitiveType = parser::Value;

pub fn parse<C: Iterator<char>>(source: C) -> Result<HashMap<String, ConfigItem>, String> {
    let mut items = HashMap::new();

    let instructions = parser::parse(source);

    for instruction in try!(instructions).move_iter() {
        try!(match instruction {
            parser::Assign(path, parser::Struct(template, instructions)) => {
                let (ty, mut fields) = try!(get_template(&items, &template), path.as_slice().connect("."));

                match evaluate(instructions, &mut fields, &items) {
                    Ok(()) => deep_insert(&mut items, path.as_slice(), Structure(ty, fields)),
                    Err(e) => return Err(format!("{}: {}", path.as_slice().connect("."), e))
                }
            },
            parser::Assign(path, parser::List(elements)) => {
                let elements = try!(evaluate_list(elements, &items), path.as_slice().connect("."));

                deep_insert(&mut items, path.as_slice(), List(elements))
            },
            parser::Assign(path, primitive) => deep_insert(&mut items, path.as_slice(), Primitive(primitive))
        })
    }

    Ok(items)
}

fn evaluate(instructions: Vec<parser::Action>, scope: &mut HashMap<String, ConfigItem>, context: &HashMap<String, ConfigItem>) -> Result<(), String> {
    for instruction in instructions.move_iter() {
        try!(match instruction {
            parser::Assign(path, parser::Struct(template, instructions)) => {
                let (ty, mut fields) = try!(get_template(context, &template), path.as_slice().connect("."));

                match evaluate(instructions, &mut fields, context) {
                    Ok(()) => deep_insert(scope, path.as_slice(), Structure(ty, fields)),
                    Err(e) => return Err(format!("{}: {}", path.as_slice().connect("."), e))
                }
            },
            parser::Assign(path, parser::List(elements)) => {
                let elements = try!(evaluate_list(elements, context), path.as_slice().connect("."));

                deep_insert(scope, path.as_slice(), List(elements))
            },
            parser::Assign(path, primitive) => deep_insert(scope, path.as_slice(), Primitive(primitive))
        })
    }

    Ok(())
}

fn evaluate_list(elements: Vec<parser::Value>, context: &HashMap<String, ConfigItem>) -> Result<Vec<ConfigItem>, String> {
    let mut result = Vec::new();
    for (i, v) in elements.move_iter().enumerate() {
        match v {
            parser::Struct(template, instructions) => {
                let (ty, mut fields) = try!(get_template(context, &template), format!("[{}]", i));

                match evaluate(instructions, &mut fields, context) {
                    Ok(()) => result.push(Structure(ty, fields)),
                    Err(e) => return Err(format!("[{}]: {}", i, e))
                }
            },
            parser::List(elements) => result.push(List(try!(evaluate_list(elements, context), format!("[{}]", i)))),
            primitive => result.push(Primitive(primitive))
        }
    }

    Ok(result)
}

fn get_template(context: &HashMap<String, ConfigItem>, template: &Vec<String>) -> Result<(Option<(String, String)>, HashMap<String, ConfigItem>), String> {
    match deep_find(context, template).map(|v| v.map(|v| (*v).clone())) {
        Ok(Some(Structure(template_type, fields))) => Ok((template_type, fields)),
        Ok(None) => match template.as_slice() {
            [ref group_name, ref type_name] => Ok((Some((group_name.clone(), type_name.clone())), HashMap::new())),
            [] => Ok((None, HashMap::new())),
            _ => Err(format!("'{}' is not a valid type name", template.as_slice().connect(".")))
        },
        Ok(Some(_)) => Err(String::from_str("only a structure or a type can be used as a template")),
        Err(e) => Err(e)
    }
}

fn deep_insert(items: &mut HashMap<String, ConfigItem>, path: &[String], item: ConfigItem) -> Result<(), String> {
    match path {
        [ref segment] => {
            items.insert(segment.clone(), item);
            Ok(())
        },
        [ref segment, ..rest] => {
            match items.find_or_insert_with(segment.clone(), |_| Structure(None, HashMap::new()) ) {
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
    groups: HashMap<String, HashMap<String, Box<Any>>>
}

impl ConfigContext {
    pub fn new() -> ConfigContext {
        ConfigContext {
            groups: HashMap::new()
        }
    }

    pub fn insert_type<T: 'static, Gr: StrAllocating, Ty: StrAllocating>(&mut self, group_name: Gr, type_name: Ty, decoder: DecoderFn<T>) -> bool {
        let group_name = group_name.into_string();
        let type_name = type_name.into_string();

        self.groups.find_or_insert_with(group_name, |_| HashMap::new()).insert(type_name, box Decoder(decoder) as Box<Any>)
    }

    pub fn decode<T: 'static + FromConfig>(&self, item: ConfigItem) -> Result<T, String> {
        match item {
            Structure(Some((group_name, type_name)), fields) => self.decode_structure(group_name, type_name, fields),
            Structure(None, fields) => FromConfig::from_structure(None, fields),
            Primitive(value) => FromConfig::from_primitive(value),
            List(list) => FromConfig::from_list(list)
        }
    }

    pub fn decode_structure_from_group<T: 'static, Gr: StrAllocating>(&self, group_name: Gr, item: ConfigItem) -> Result<T, String> {
        let group_name = group_name.into_string();

        match item {
            Structure(Some((item_group_name, type_name)), fields) => if group_name == item_group_name {
                self.decode_structure(group_name, type_name, fields)
            } else {
                Err(format!("expected a structure from group '{}', but found '{}.{}'", group_name, item_group_name, type_name))
            },
            Structure(None, _) => Err(format!("expected a structure from group '{}', but found an untyped structure", group_name)),
            Primitive(value) => Err(format!("expected a structure from group '{}', but found '{}'", group_name, value)),
            List(_) => Err(format!("expected a structure from group '{}', but found a list", group_name))
        }
    }

    pub fn decode_structure_of_type<T: 'static, Gr: StrAllocating, Ty: StrAllocating>(&self, group_name: Gr, type_name: Ty, item: ConfigItem) -> Result<T, String> {
        let group_name = group_name.into_string();
        let type_name = type_name.into_string();

        match item {
            Structure(Some((item_group_name, item_type_name)), fields) =>{
                if group_name == item_group_name && type_name == item_type_name {
                    self.decode_structure(group_name, type_name, fields)
                } else {
                    Err(format!("expected a structure of type '{}.{}', but found '{}.{}'", group_name, type_name, item_group_name, item_type_name))
                }
            },
            Structure(None, _) => Err(format!("expected a structure of type '{}.{}', but found an untyped structure", group_name, type_name)),
            Primitive(value) => Err(format!("expected a structure of type '{}.{}', but found '{}'", group_name, type_name, value)),
            List(_) => Err(format!("expected a structure of type '{}.{}', but found a list", group_name, type_name))
        }
    }

    pub fn decode_structure<T: 'static, Gr: StrAllocating, Ty: StrAllocating>(&self, group_name: Gr, type_name: Ty, fields: HashMap<String, ConfigItem>) -> Result<T, String> {
        let group_name = group_name.into_string();
        let type_name = type_name.into_string();

        match self.groups.find_equiv(&group_name).and_then(|group| group.find_equiv(&type_name)) {
            Some(decoder) => match decoder.downcast_ref::<Decoder<T>>() {
                Some(decoder) => decoder.decode(self, fields),
                None => Err(format!("type cannot be decoded from '{}.{}'", group_name, type_name))
            },
            None => Err(format!("unknown type '{}.{}'", group_name, type_name))
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



#[deriving(Clone)]
pub enum ConfigItem {
    Structure(Option<(String, String)>, HashMap<String, ConfigItem>),
    List(Vec<ConfigItem>),
    Primitive(parser::Value)
}

impl ConfigItem {
    pub fn into_float(self) -> Option<f64> {
        match self {
            Primitive(parser::Number(f)) => Some(f),
            _ => None
        }
    }

    pub fn is_float(&self) -> bool {
        match self {
            &Primitive(parser::Number(_)) => true,
            _ => false
        }
    }

    pub fn into_string(self) -> Option<String> {
        match self {
            Primitive(parser::String(s)) => Some(s),
            _ => None
        }
    }

    pub fn is_string(&self) -> bool {
        match self {
            &Primitive(parser::String(_)) => true,
            _ => false
        }
    }

    pub fn into_fields(self) -> Option<HashMap<String, ConfigItem>> {
        match self {
            Structure(_, fields) => Some(fields),
            _ => None
        }
    }

    pub fn is_structure(&self) -> bool {
        match self {
            &Structure(..) => true,
            _ => false
        }
    }

    pub fn into_list(self) -> Result<Vec<ConfigItem>, String> {
        match self {
            List(v) => Ok(v),
            Structure(Some((group_name, type_name)), _) => Err(format!("expected a list, but found a structure of type {}.{}", group_name, type_name)),
            Structure(None, _) => Err(String::from_str("expected a list, but found an untyped structure")),
            Primitive(v) => Err(format!("expected a list, but found {}", v))
        }
    }
}

pub trait FromConfig {
    fn from_primitive(item: PrimitiveType) -> Result<Self, String> {
        Err(format!("unexpected {}", item))
    }

    fn from_structure(structure_type: Option<(String, String)>, _fields: HashMap<String, ConfigItem>) -> Result<Self, String> {
        match structure_type {
            Some((group_name, type_name)) => Err(format!("unexpected structure of type {}.{}", group_name, type_name)),
            None => Err(String::from_str("unexpected untyped structure"))
        }
    }

    fn from_list(_elements: Vec<ConfigItem>) -> Result<Self, String> {
        Err(format!("unexpected list"))
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