//!A parser and decoder for a human friendly configuration language.

extern crate anymap;
extern crate lalrpop_util;

mod ast;
mod parser;
pub mod entry;
pub mod prelude;

use std::path::Path;
use std::fs::File;
use std::io::Read;
use std::collections::HashMap;
use std::fmt;

use anymap::AnyMap;
use anymap::any::Any;

use entry::Entry;

pub use ast::Number;
pub use prelude::Prelude;

///A collective parsing and interpretation error.
#[derive(Debug)]
pub enum Error {
    ///Failed to parse the source.
    Parse(parser::Error),
    ///Failed to read a file.
    Io(std::io::Error),
    ///Something should be an object, but it wasn't.
    NotAnObject(StackTrace),
    ///A circular reference was discovered.
    CircularReference(StackTrace),
    ///Too many arguments was passed while extending an object.
    TooManyArguments(StackTrace, usize),
    ///Something was reassigned.
    Reassign(StackTrace),
    ///An attempt to refer to a local value in a list.
    LocalPathInList(StackTrace, Vec<String>)
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            Error::Parse(ref e) => write!(f, "error while parsing: {:?}", e),
            Error::Io(ref e) => write!(f, "IO error: {:?}", e),
            Error::NotAnObject(ref p) => write!(f, "{} is not an object", p),
            Error::CircularReference(ref p) => write!(f, "circular reference detected in {}", p),
            Error::TooManyArguments(ref p, len) => write!(f, "too many arguments in {}. {} were expected", p, len),
            Error::Reassign(ref p) => write!(f, "{} cannot be reassigned", p),
            Error::LocalPathInList(ref trace, ref p) => write!(f, "the item {:?} cannot be accessed from within the list {}", p, trace),
        }
    }
}

impl From<parser::Error> for Error {
    fn from(e: parser::Error) -> Error {
        Error::Parse(e)
    }
}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Error {
        Error::Io(e)
    }
}

///Parses the configuration source.
pub struct Parser {
    nodes: Vec<Node>,
    prelude: HashMap<String, usize>
}

impl Parser {
    ///Create a new parser.
    pub fn new() -> Parser {
        Parser {
            nodes: vec![Node::new(
                NodeType::Object {
                    base: None,
                    children: HashMap::new(),
                    arguments: vec![]
                },
                0
            )],
            prelude: HashMap::new()
        }
    }

    ///Parse a file.
    pub fn parse_file<P: AsRef<Path>>(&mut self, path: P) -> Result<(), Error> {
        parse_file_in(path, Object::root_of(self), None)
    }

    ///Parse a string.
    pub fn parse_string(&mut self, source: &str) -> Result<(), Error> {
        parse(".", source, Object::root_of(self))
    }

    ///Get a reference to the root entry.
    pub fn root(&self) -> Entry {
        Entry::root_of(self)
    }

    fn add_node(&mut self, node: Node) -> usize {
        self.nodes.push(node);
        self.nodes.len() - 1
    }

    fn get_concrete_node(&self, template: usize) -> &Node {
        let mut current_template = template;
        loop {
            let node = &self.nodes[current_template];
            current_template = match node.ty {
                NodeType::Link(t) => t,
                _ => return node
            };
        }
    }

    fn get_concrete_node_mut(&mut self, template: usize) -> &mut Node {
        let mut current_template = template;
        loop {
            match self.nodes[current_template].ty {
                NodeType::Link(t) => current_template = t,
                _ => break
            }
        }

        &mut self.nodes[current_template]
    }

    fn get_decoder<T: Any>(&self, id: usize) -> Option<&T> {
        let mut current_id = id;
        loop {
            let node = &self.nodes[current_id];

            if let Some(decoder) = node.decoder.get() {
                return Some(decoder);
            }

            current_id = match node.ty {
                NodeType::Link(t) => t,
                NodeType::Object { base: Some(b), .. } => b,
                _ => return None
            };
        }
    }

    fn infer_object(&mut self, id: usize) -> bool {
        match &mut self.get_concrete_node_mut(id).ty {
            s @ &mut NodeType::Unknown => {
                *s = NodeType::Object {
                    base: None,
                    children: HashMap::new(),
                    arguments: vec![]
                };
                true
            },
            &mut NodeType::Object { .. } => true,
            _ => false
        }
    }

    fn cloned_arguments(&self, id: usize) -> Option<Vec<String>> {
        let mut current = self.get_concrete_node(id);
        loop {
            match current.ty {
                NodeType::Object { ref arguments, .. } if arguments.len() > 0 => return Some(arguments.clone()),
                NodeType::Object { base: Some(id), ..} | NodeType::Link(id) => current = &self.nodes[id],
                NodeType::Object { base: None, .. } => return Some(vec![]),
                _ => return None
            }
        }
    }

    fn find_child_id(&self, parent: usize, key: &str) -> Option<usize> {
        let mut node = parent;

        loop {
            match self.nodes[node].ty {
                NodeType::Object { base, ref children, .. } => {
                    if let Some(&child) = children.get(key) {
                        return Some(child);
                    } else if let Some(base) = base {
                        node = base;
                    } else {
                        return None;
                    }
                },
                NodeType::Link(base) => node = base,
                _ => return None
            }
        }
    }

    fn trace(&self, mut id: usize) -> StackTrace {
        let mut stack = vec![];

        while id != 0 {
            let parent = self.nodes[id].parent;
            match self.nodes[parent].ty {
                NodeType::Object { ref children, ..} => {
                    for (ident, &child) in children {
                        if child == id {
                            stack.push(Selection::Ident(ident.clone()));
                            break;
                        }
                    }
                },
                NodeType::List(ref list) => {
                    let i = list.iter().position(|&child| child == id).expect("child not found in parent list");
                    stack.push(Selection::Index(i));
                },
                _ => panic!("non-object and non-list is set as parent")
            }
            id = parent;
        }

        stack.reverse();
        StackTrace(stack)
    }

    fn find_in_prelude(&self, path: &[String]) -> Option<usize> {
        let mut path = path.iter();
        let mut current_id = path.next().and_then(|key| self.prelude.get(key).map(|&id| id));

        while let (Some(id), Some(key)) = (current_id, path.next()) {
            if let NodeType::Object { ref children, .. } = self.nodes[id].ty {
                current_id = children.get(key).map(|&child| child);
            } else {
                return None
            }
        }

        current_id
    }

    fn make_object(&mut self, id: usize) -> Result<(), Error> {
        if !self.infer_object(id) {
            return Err(Error::NotAnObject(self.trace(id)));
        }

        let upgrade = if let NodeType::Link(base) = self.nodes[id].ty {
            Some(base)
        } else {
            None
        };

        if let Some(base) = upgrade {
            self.nodes[id].ty = NodeType::Object {
                base: Some(base),
                children: HashMap::new(),
                arguments: vec![]
            }
        }
        Ok(())
    }

    fn links_to(&self, link_id: usize, target_id: usize) -> bool {
        let mut current = link_id;
        while current != target_id {
            current = match self.nodes[current].ty {
                NodeType::Object { base: Some(t), .. } |
                NodeType::Link(t) => t,
                _ => return false
            };
        }

        true
    }
}

fn parse_file_in<P: AsRef<Path>>(path: P, mut root: Object, new_root: Option<ast::Path>) -> Result<(), Error> {
    let mut source = String::new();
    let mut file = try!(File::open(&path));
    try!(file.read_to_string(&mut source));

    let new_root = if let Some(new_root) = new_root {
        try!(root.object(new_root.path)).into_root()
    } else {
        root
    };

    parse(path.as_ref().parent().unwrap(), &source, new_root)
}

fn parse<P: AsRef<Path>>(path: P, source: &str, mut root: Object) -> Result<(), Error> {
    let statements = try!(parser::parse(source));

    for statement in statements {
        /*let parser::Span {
            item: statement,
            ..
        } = statement;*/

        match statement {
            ast::Statement::Include(file, new_root) => {
                let path = path.as_ref().join(file);
                try!(parse_file_in(path, root.borrow(), new_root))
            },
            ast::Statement::Assign(path, value) => try!(root.assign(path.path, value))
        }
    }

    Ok(())
}

///A trait for things that can be dynamically decoded.
pub trait Decode: Any {}

impl<T: Any> Decode for T {}

struct Decoder<T: Decode>(Box<Fn(Entry) -> Result<T, String>>);

impl<T: Decode> Decoder<T> {
    fn new<F>(decode_fn: F) -> Decoder<T> where
        F: Fn(Entry) -> Result<T, String>,
        F: 'static
    {
        Decoder(Box::new(decode_fn))
    }
}

#[derive(Debug)]
struct Node {
    ty: NodeType,
    parent: usize,
    decoder: AnyMap
}

impl Node {
    fn new(ty: NodeType, parent: usize) -> Node {
        Node {
            ty: ty,
            parent: parent,
            decoder: AnyMap::new()
        }
    }
}

impl PartialEq for Node {
    fn eq(&self, other: &Node) -> bool {
        self.ty == other.ty && self.parent == other.parent
    }
}

#[derive(PartialEq, Debug)]
enum NodeType {
    Unknown,
    Link(usize),
    Object {
        base: Option<usize>,
        children: HashMap<String, usize>,
        arguments: Vec<String>,
    },
    Value(Value),
    List(Vec<usize>)
}

macro_rules! impl_value_from_float {
    ($($ty: ty),+) => (
        $(impl From<$ty> for Value {
            fn from(v: $ty) -> Value {
                Value::Number(Number::Float(v as f64))
            }
        })+
    )
}

macro_rules! impl_value_from_int {
    ($($ty: ty),+) => (
        $(impl From<$ty> for Value {
            fn from(v: $ty) -> Value {
                Value::Number(Number::Integer(v as i64))
            }
        })+
    )
}

///A primitive value.
#[derive(PartialEq, Debug)]
pub enum Value {
    ///A float or an int.
    Number(Number),
    ///A string.
    String(String)
}

impl_value_from_float!(f32, f64);
impl_value_from_int!(i8, i16, i32, i64, u8, u16, u32);

impl From<String> for Value {
    fn from(v: String) -> Value {
        Value::String(v)
    }
}

#[derive(Clone, Debug)]
enum Selection {
    Ident(String),
    Index(usize)
}

///The path to something in the configuration.
///
///It's only for printing purposes.
#[derive(Debug)]
pub struct StackTrace(Vec<Selection>);

impl fmt::Display for StackTrace {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let mut path = self.0.iter();
        match path.next() {
            Some(&Selection::Ident(ref ident)) => try!(ident.fmt(f)),
            Some(&Selection::Index(index)) => try!(write!(f, "{{root}}[{}]", index)),
            None => try!("{root}".fmt(f))
        }

        for s in path {
            match *s {
                Selection::Ident(ref ident) => try!(write!(f, ".{}", ident)),
                Selection::Index(index) => try!(write!(f, "[{}]", index)),
            }
        }

        Ok(())
    }
}

struct Object<'a> {
    cfg: &'a mut Parser,
    id: usize,
    root: usize,
}

impl<'a> Object<'a> {
    fn root_of(cfg: &mut Parser) -> Object {
        Object {
            cfg: cfg,
            id: 0,
            root: 0
        }
    }

    fn root(&mut self) -> Object {
        Object {
            cfg: self.cfg,
            id: self.root,
            root: self.root
        }
    }

    fn into_root(self) -> Object<'a> {
        Object {
            cfg: self.cfg,
            id: self.id,
            root: self.id,
        }
    }

    fn borrow(&mut self) -> Object {
        Object {
            cfg: self.cfg,
            id: self.id,
            root: self.root
        }
    }

    fn set_base(&mut self, base_id: usize) -> Result<(), Error> {
        if self.cfg.infer_object(base_id) {
            if let NodeType::Object { ref mut base, .. } = self.cfg.nodes[self.id].ty {
                *base = Some(base_id);
            } else {
                unreachable!();
            };
            Ok(())
        } else {
            Err(Error::NotAnObject(self.cfg.trace(base_id)))
        }
    }

    fn assign<P>(&mut self, path: P, value: ast::Value) -> Result<(), Error> where
        P: IntoIterator<Item=String>,
        P::IntoIter: Iterator,
        P::IntoIter: ExactSizeIterator
    {
        match value {
            ast::Value::Object(ast::Object::New(values)) => {
                let mut obj = try!(self.object(path));
                for (path, value) in values {
                    try!(obj.assign(path.path, value));
                }
            },
            ast::Value::Object(ast::Object::Extension(base, Some(values))) => {
                let base_id = match base.path_type {
                    ast::PathType::Local => try!(self.id_of(base.path)),
                    ast::PathType::Global => try!(self.root().id_of(base.path))
                };


                let mut obj = try!(self.object(path));
                try!(obj.set_base(base_id));

                match values {
                    ast::ExtensionChanges::BlockStyle(values) => {
                        for (path, value) in values {
                            try!(obj.assign(path.path, value));
                        }
                    },
                    ast::ExtensionChanges::FunctionStyle(values) => {
                        if let Some(arguments) = obj.arguments() {
                            if arguments.len() < values.len() {
                                return Err(Error::TooManyArguments(obj.trace(), arguments.len()));
                            } else {
                                for (field, value) in arguments.into_iter().zip(values) {
                                    try!(obj.assign(Some(field), value));
                                }
                            }
                        }
                    }
                }
            },
            ast::Value::Object(ast::Object::Extension(base, None)) => {
                let base_id = match base.path_type {
                    ast::PathType::Local => try!(self.id_of(base.path)),
                    ast::PathType::Global => try!(self.root().id_of(base.path))
                };
                try!(self.link(path, base_id));
            },
            ast::Value::Number(number) => try!(self.value(path, Value::Number(number))),
            ast::Value::String(string) => try!(self.value(path, Value::String(string))),
            ast::Value::List(values) => {
                let mut list = try!(self.list(path));
                for value in values {
                    try!(list.insert(value));
                }
            }
        }
        Ok(())
    }

    fn object<P>(&mut self, path: P) -> Result<Object, Error> where
        P: IntoIterator<Item=String>,
        P::IntoIter: Iterator,
        P::IntoIter: ExactSizeIterator
    {
        enum Action {
            Continue(usize),
            Upgrade(Option<usize>),
            Create(Option<usize>)
        }

        let path = path.into_iter();
        assert!(path.len() > 0);

        let mut obj = self.id;

        for ident in path {
            let child_action = match self.cfg.nodes[obj].ty {
                NodeType::Object { base, ref children, .. } => {
                    if let Some(id) = children.get(&ident).map(|&child| child) {
                        Action::Continue(id)
                    } else {
                        Action::Create(base.and_then(|id| self.cfg.find_child_id(id, &ident)))
                    }
                },
                NodeType::Link(base) => Action::Upgrade(Some(base)),
                NodeType::Unknown => unreachable!("{}: unknown type", self.cfg.trace(obj)),
                NodeType::List(_) => unreachable!("{}: list", self.cfg.trace(obj)),
                NodeType::Value(_) => unreachable!("{}: value", self.cfg.trace(obj)),
            };

            match child_action {
                Action::Continue(child) => {
                    if !self.cfg.infer_object(child) {
                        return Err(Error::NotAnObject(self.cfg.trace(child)));
                    }

                    obj = child;
                },
                Action::Upgrade(base) => {
                    if let Some(base) = base {
                        if !self.cfg.infer_object(base) {
                            return Err(Error::NotAnObject(self.cfg.trace(base)));
                        }
                    }

                    self.cfg.nodes[obj].ty = NodeType::Object {
                        base: base,
                        children: HashMap::new(),
                        arguments: vec![]
                    };

                    let child_base = base.and_then(|id| self.cfg.find_child_id(id, &ident));

                    let mut o = Object {
                        cfg: self.cfg,
                        id: obj,
                        root: self.root
                    };

                    obj = try!(o.add_child(ident, NodeType::Object {
                        base: child_base,
                        children: HashMap::new(),
                        arguments: vec![]
                    }));
                },
                Action::Create(base) => {
                    if let Some(base) = base {
                        if !self.cfg.infer_object(base) {
                            return Err(Error::NotAnObject(self.cfg.trace(base)));
                        }
                    }

                    let mut o = Object {
                        cfg: self.cfg,
                        id: obj,
                        root: self.root
                    };

                    obj = try!(o.add_child(ident, NodeType::Object {
                        base: base,
                        children: HashMap::new(),
                        arguments: vec![]
                    }));
                }
            }
        }

        try!(self.cfg.make_object(obj));

        Ok(Object {
            cfg: self.cfg,
            id: obj,
            root: self.root,
        })
    }

    fn link<P>(&mut self, path: P, base_id: usize) -> Result<(), Error> where
        P: IntoIterator<Item=String>,
        P::IntoIter: Iterator,
        P::IntoIter: ExactSizeIterator
    {
        let mut path = path.into_iter();
        let len = path.len();
        assert!(len > 0);

        let mut obj = if len > 1 {
            try!(self.object(path.by_ref().take(len - 1)))
        } else {
            self.borrow()
        };
        let key = path.next().unwrap();
        obj.add_child(key, NodeType::Link(base_id)).map(|_| ())
    }

    fn value<P>(&mut self, path: P, value: Value) -> Result<(), Error> where
        P: IntoIterator<Item=String>,
        P::IntoIter: Iterator,
        P::IntoIter: ExactSizeIterator
    {
        let mut path = path.into_iter();
        let len = path.len();
        assert!(len > 0);

        let mut obj = if len > 1 {
            try!(self.object(path.by_ref().take(len - 1)))
        } else {
            self.borrow()
        };
        let key = path.next().unwrap();
        obj.add_child(key, NodeType::Value(value)).map(|_| ())
    }

    fn list<P>(&mut self, path: P) -> Result<List, Error> where
        P: IntoIterator<Item=String>,
        P::IntoIter: Iterator,
        P::IntoIter: ExactSizeIterator
    {
        let mut path = path.into_iter();
        let len = path.len();
        assert!(len > 0);

        let id = {
            let mut obj = if len > 1 {
                try!(self.object(path.by_ref().take(len - 1)))
            } else {
                self.borrow()
            };
            let key = path.next().unwrap();
            try!(obj.add_child(key, NodeType::List(vec![])))
        };
        Ok(List {
            cfg: self.cfg,
            id: id,
            root: self.root
        })
    }

    fn add_child(&mut self, key: String, ty: NodeType) -> Result<usize, Error> {
        let current_child = match self.cfg.nodes[self.id].ty {
            NodeType::Object { ref mut children, .. } => children.get(&key).map(|&child| child),
            NodeType::Link(_) => unreachable!("(id: {}) why am I a link?", self.id),
            NodeType::Unknown => unreachable!("(id: {}) why am I an unknown type?", self.id),
            NodeType::List(_) => unreachable!("(id: {}) why am I a list?", self.id),
            NodeType::Value(_) => unreachable!("(id: {}) why am I a value?", self.id),
        };

        if let Some(id) = current_child {
            match ty {
                NodeType::Object { base: Some(base_id), .. } |
                NodeType::Link(base_id) => if self.cfg.links_to(base_id, id) {
                    return Err(Error::CircularReference(self.cfg.trace(id)));
                },
                _ => {}
            }

            if let current_ty @ &mut NodeType::Unknown = &mut self.cfg.nodes[id].ty {

                *current_ty = ty;
                Ok(id)
            } else {
                Err(())
            }.map_err(|_| Error::Reassign(self.cfg.trace(id)))
        } else {
            let node = Node::new(ty, self.id);
            let id = self.cfg.add_node(node);
            if let NodeType::Object { ref mut children, .. } = self.cfg.nodes[self.id].ty {
                children.insert(key, id);
            } else {
                unreachable!();
            }

            Ok(id)
        }

    }

    fn id_of(&mut self, path: Vec<String>) -> Result<usize, Error> {
        let maybe_prelude = self.cfg.find_in_prelude(&path);

        if let Some(id) = maybe_prelude {
            Ok(id)
        } else {
            let mut path = path.into_iter();
            let len = path.len();
            assert!(len > 0);

            let mut obj = if len > 1 {
                try!(self.object(path.by_ref().take(len - 1)))
            } else {
                self.borrow()
            };
            
            let key = path.next().unwrap();
            let current_child = if let NodeType::Object { ref mut children, .. } = obj.cfg.nodes[obj.id].ty {
                children.get(&key).map(|&child| child)
            } else {
                unreachable!();
            };

            if let Some(id) = current_child {
                Ok(id)
            } else {
                obj.add_child(key, NodeType::Unknown)
            }
        }
    }

    fn arguments(&self) -> Option<Vec<String>> {
        self.cfg.cloned_arguments(self.id)
    }

    fn trace(&self) -> StackTrace {
        self.cfg.trace(self.id)
    }
}

struct List<'a> {
    cfg: &'a mut Parser,
    id: usize,
    root: usize
}

impl<'a> List<'a> {
    fn root(&mut self) -> Object {
        Object {
            cfg: self.cfg,
            id: self.root,
            root: self.root
        }
    }

    fn insert(&mut self, value: ast::Value) -> Result<(), Error> {
        match value {
            ast::Value::Object(ast::Object::New(values)) => {
                let mut obj = self.object();
                for (path, value) in values {
                    try!(obj.assign(path.path, value));
                }
            },
            ast::Value::Object(ast::Object::Extension(base, Some(values))) => {
                let base_id = match base.path_type {
                    ast::PathType::Local => return Err(Error::LocalPathInList(self.trace(), base.path)),
                    ast::PathType::Global => try!(self.root().id_of(base.path))
                };

                let mut obj = self.object();
                try!(obj.set_base(base_id));

                match values {
                    ast::ExtensionChanges::BlockStyle(values) => {
                        for (path, value) in values {
                            try!(obj.assign(path.path, value));
                        }
                    },
                    ast::ExtensionChanges::FunctionStyle(values) => {
                        if let Some(arguments) = obj.arguments() {
                            if arguments.len() < values.len() {
                                return Err(Error::TooManyArguments(obj.trace(), arguments.len()));
                            } else {
                                for (field, value) in arguments.into_iter().zip(values) {
                                    try!(obj.assign(Some(field), value));
                                }
                            }
                        }
                    }
                }
            },
            ast::Value::Object(ast::Object::Extension(base, None)) => {
                let base_id = match base.path_type {
                    ast::PathType::Local => return Err(Error::LocalPathInList(self.trace(), base.path)),
                    ast::PathType::Global => try!(self.root().id_of(base.path))
                };
                self.link(base_id);
            },
            ast::Value::Number(number) => self.value(Value::Number(number)),
            ast::Value::String(string) => self.value(Value::String(string)),
            ast::Value::List(values) => {
                let mut list = self.list();
                for value in values {
                    try!(list.insert(value));
                }
            }
        }
        Ok(())
    }

    fn object(&mut self) -> Object {
        let id = self.add_item(NodeType::Object {
            base: None,
            children: HashMap::new(),
            arguments: vec![]
        });
        Object {
            cfg: self.cfg,
            id: id,
            root: self.root
        }
    }

    fn link(&mut self, base_id: usize) {
        self.add_item(NodeType::Link(base_id));
    }

    fn value(&mut self, value: Value) {
        self.add_item(NodeType::Value(value));
    }

    fn list(&mut self) -> List {
        let id = self.add_item(NodeType::List(vec![]));
        List {
            cfg: self.cfg,
            id: id,
            root: self.root
        }
    }

    fn add_item(&mut self, ty: NodeType) -> usize {
        let node = Node::new(ty, self.id);
        let id = self.cfg.add_node(node);
        if let NodeType::List(ref mut items) = self.cfg.nodes[self.id].ty {
            items.push(id);
        } else {
            unreachable!();
        };

        id
    }

    fn trace(&self) -> StackTrace {
        self.cfg.trace(self.id)
    }
}


#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use anymap::any::Any;
    use Parser;
    use Prelude;
    use Node;
    use NodeType;
    use Object;
    use entry::{Entry, FromEntry};

    macro_rules! assert_ok {
        ($e: expr) => (if let Err(e) = $e {
            const EXPR: &'static str = stringify!($e);
            panic!("{} failed with error: {}", EXPR, e);
        })
    }

    #[derive(PartialEq, Debug)]
    struct SingleItem<T>(T);

    #[derive(PartialEq, Debug)]
    struct SingleItem2<T>(T);

    impl<'a, T: FromEntry<'a>> FromEntry<'a> for SingleItem<T> {
        fn from_entry(entry: Entry<'a>) -> Result<SingleItem<T>, String> {
            entry.as_object().ok_or("expected an object".into())
                .and_then(|o| o.get("a").ok_or("missing field a".into()))
                .and_then(|e| T::from_entry(e))
                .map(|item| SingleItem(item))
        }
    }

    impl<'a, T: FromEntry<'a>> FromEntry<'a> for SingleItem2<T> {
        fn from_entry(entry: Entry<'a>) -> Result<SingleItem2<T>, String> {
            entry.as_object().ok_or("expected an object".into())
                .and_then(|o| o.get("a").ok_or("missing field a".into()))
                .and_then(|e| T::from_entry(e))
                .map(|item| SingleItem2(item))
        }
    }

    fn with_prelude<T: for<'a> FromEntry<'a> + Any>() -> Parser {
        let mut prelude = Prelude::new();
        {
            let mut object = prelude.object("o".into());
            object.add_decoder(|entry| SingleItem::<T>::from_entry(entry));
            object.add_decoder(|entry| SingleItem2::<T>::from_entry(entry));
        }
        prelude.into_parser()
    }

    fn with_args<T: for<'a> FromEntry<'a> + Any>() -> Parser {
        let mut prelude = Prelude::new();
        {
            let mut object = prelude.object("o".into());
            object.object("a".into());
            object.object("b".into());
            object.object("c".into());
            object.arguments(vec!["a".into(), "b".into(), "c".into()]);
        }
        prelude.into_parser()
    }

    #[test]
    fn assign_object() {
        let mut cfg = Parser::new();
        cfg.parse_string("a = {}").unwrap();
        assert_eq!(cfg.nodes, vec![
            Node::new(
                NodeType::Object {
                    base: None,
                    children: vec![("a".into(), 1)].into_iter().collect(),
                    arguments: vec![]
                },
                0
            ),
            Node::new(
                NodeType::Object {
                    base: None,
                    children: HashMap::new(),
                    arguments: vec![]
                },
                0
            )
        ]);
    }

    #[test]
    fn assign_deep_object() {
        let mut cfg = Parser::new();
        cfg.parse_string("a.b = {}").unwrap();
        assert_eq!(cfg.nodes, vec![
            Node::new(
                NodeType::Object {
                    base: None,
                    children: vec![("a".into(), 1)].into_iter().collect(),
                    arguments: vec![]
                },
                0
            ),
            Node::new(
                NodeType::Object {
                    base: None,
                    children: vec![("b".into(), 2)].into_iter().collect(),
                    arguments: vec![]
                },
                0
            ),
            Node::new(
                NodeType::Object {
                    base: None,
                    children: HashMap::new(),
                    arguments: vec![]
                },
                1
            )
        ]);
    }

    #[test]
    fn assign_in_object() {
        let mut cfg = Parser::new();
        cfg.parse_string("a = { b = {} }").unwrap();
        assert_eq!(cfg.nodes, vec![
            Node::new(
                NodeType::Object {
                    base: None,
                    children: vec![("a".into(), 1)].into_iter().collect(),
                    arguments: vec![]
                },
                0
            ),
            Node::new(
                NodeType::Object {
                    base: None,
                    children: vec![("b".into(), 2)].into_iter().collect(),
                    arguments: vec![]
                },
                0
            ),
            Node::new(
                NodeType::Object {
                    base: None,
                    children: HashMap::new(),
                    arguments: vec![]
                },
                1
            )
        ]);
    }

    #[test]
    fn assign_to_object() {
        let mut cfg = Parser::new();
        cfg.parse_string("a = {}").unwrap();
        assert_eq!(cfg.nodes, vec![
            Node::new(
                NodeType::Object {
                    base: None,
                    children: vec![("a".into(), 1)].into_iter().collect(),
                    arguments: vec![]
                },
                0
            ),
            Node::new(
                NodeType::Object {
                    base: None,
                    children: HashMap::new(),
                    arguments: vec![]
                },
                0
            )
        ]);

        cfg.parse_string("a.b = {}").unwrap();
        assert_eq!(cfg.nodes, vec![
            Node::new(
                NodeType::Object {
                    base: None,
                    children: vec![("a".into(), 1)].into_iter().collect(),
                    arguments: vec![]
                },
                0
            ),
            Node::new(
                NodeType::Object {
                    base: None,
                    children: vec![("b".into(), 2)].into_iter().collect(),
                    arguments: vec![]
                },
                0
            ),
            Node::new(
                NodeType::Object {
                    base: None,
                    children: HashMap::new(),
                    arguments: vec![]
                },
                1
            )
        ]);
    }

    #[test]
    fn extend_block_style() {
        let mut cfg = Parser::new();
        cfg.parse_string("a = {} b = a { c = {} }").unwrap();
        assert_eq!(cfg.nodes, vec![
            Node::new(
                NodeType::Object {
                    base: None,
                    children: vec![("a".into(), 1),
                    ("b".into(), 2)].into_iter().collect(),
                    arguments: vec![]
                },
                0
            ),
            Node::new(
                NodeType::Object {
                    base: None,
                    children: HashMap::new(),
                    arguments: vec![]
                },
                0
            ),
            Node::new(
                NodeType::Object {
                    base: Some(1),
                    children: vec![("c".into(), 3)].into_iter().collect(),
                    arguments: vec![]
                },
                0
            ),
            Node::new(
                NodeType::Object {
                    base: None,
                    children: HashMap::new(),
                    arguments: vec![]
                },
                2
            ),
        ]);
    }

    #[test]
    fn insert_list() {
        let mut cfg = Parser::new();
        cfg.parse_string("a = []").unwrap();
        assert_eq!(cfg.nodes, vec![
            Node::new(
                NodeType::Object {
                    base: None,
                    children: vec![("a".into(), 1)].into_iter().collect(),
                    arguments: vec![]
                },
                0
            ),
            Node::new(
                NodeType::List(vec![]),
                0
            )
        ])
    }

    #[test]
    fn extend_list() {
        let mut cfg = Parser::new();
        assert!(cfg.parse_string("a = [] b = a { c = {} }").is_err());
    }

    #[test]
    fn link_to_list() {
        let mut cfg = Parser::new();
        assert_ok!(cfg.parse_string("a = [] b = a"));
    }

    #[test]
    fn decode_integer_list() {
        let mut cfg = Parser::new();
        assert_ok!(cfg.parse_string("a = [1, 2, 3]"));
        assert_eq!(Ok(SingleItem(vec![1, 2, 3])), cfg.root().decode());
    }

    #[test]
    fn decode_float_list() {
        let mut cfg = Parser::new();
        assert_ok!(cfg.parse_string("a = [1.0, -2.4, 3.8]"));
        assert_eq!(Ok(SingleItem(vec![1.0, -2.4, 3.8])), cfg.root().decode());
    }

    #[test]
    fn decode_string_list() {
        let mut cfg = Parser::new();
        assert_ok!(cfg.parse_string("a = [\"foo\", \"bar\"]"));
        assert_eq!(Ok(SingleItem(vec!["foo", "bar"])), cfg.root().decode());
    }

    #[test]
    fn dynamic_decode_integer_list() {
        let mut cfg = with_prelude::<Vec<i32>>();
        assert_ok!(cfg.parse_string("b = o { a = [1, 2, 3] }"));
        let b = cfg.root().get("b");
        assert_eq!(Ok(SingleItem(vec![1, 2, 3])), b.dynamic_decode());
        assert_eq!(Ok(SingleItem2(vec![1, 2, 3])), b.dynamic_decode());
    }

    #[test]
    fn parse_relative_in_inner_root() {
        let mut cfg = Parser::new();
        assert_ok!(cfg.parse_string("a.b.c = {}"));
        {
            let root_id = cfg.nodes.len() - 1;
            let root = Object {
                cfg: &mut cfg,
                id: root_id,
                root: root_id
            };
            assert_ok!(super::parse(".", "d = 1", root));
        }
        assert_eq!(Ok(1), cfg.root().get("a").get("b").get("c").get("d").decode());
    }

    #[test]
    fn link_in_inner_root() {
        let mut cfg = Parser::new();
        assert_ok!(cfg.parse_string("a.b.c = {} f = []"));
        {
            let root_id = cfg.nodes.len() - 2;
            let root = Object {
                cfg: &mut cfg,
                id: root_id,
                root: root_id
            };
            assert_ok!(super::parse(".", "d = f{}", root));
        }
        assert!(cfg.parse_string("d = f{}").is_err());
    }

    #[test]
    fn function_style() {
        let mut cfg = with_args::<i32>();
        assert_ok!(cfg.parse_string("a = o(1, 2, 3)"));
        assert_ok!(cfg.parse_string("b = o(1, 2)"));
        assert!(cfg.parse_string("c = o(1, 2, 3, 4)").is_err());
    }

    #[test]
    fn extend_upgrade_decode() {
        let mut cfg = with_prelude::<Vec<i32>>();
        assert_ok!(cfg.parse_string("b = o b.a = [1, 2, 3]"));
        let b = cfg.root().get("b");
        assert_eq!(Ok(SingleItem(vec![1, 2, 3])), b.dynamic_decode());
        assert_eq!(Ok(SingleItem2(vec![1, 2, 3])), b.dynamic_decode());
    }

    #[test]
    fn extend_upgrade_unknown() {
        let mut cfg = Parser::new();
        assert_ok!(cfg.parse_string("a = b.c b.c = 5"));
        let a = cfg.root().get("a");
        assert_eq!(Ok(5), a.decode());
    }

    #[test]
    fn circular_reference() {
        let mut cfg = Parser::new();
        assert!(cfg.parse_string("a = b b = a").is_err());
        assert!(cfg.parse_string("a = b b = c c = a").is_err());
        assert!(cfg.parse_string("a = b b = c c = a { b = 0}").is_err());
    }
}
