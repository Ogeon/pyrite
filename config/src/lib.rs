extern crate anymap;
extern crate lalrpop_util;

mod ast;
mod parser;
pub mod entry;
pub mod prelude;

use std::path::Path;
use std::fs::File;
use std::io::Read;
use std::ops::{Deref, DerefMut};
use std::collections::HashMap;

use anymap::AnyMap;
use anymap::any::Any;

use entry::Entry;

pub use ast::Number;
pub use prelude::Prelude;

#[derive(Debug)]
pub enum Error {
    Parse(parser::Error),
    Io(std::io::Error),
    NonObjectTemplate(Vec<Selection>),
    NotAnObject(Vec<Selection>),
    CircularReference(Vec<Selection>)
}

impl Error {
    fn set_path(&mut self, path: Vec<Selection>) {
        match *self {
            Error::NonObjectTemplate(ref mut p) |
            Error::NotAnObject(ref mut p) |
            Error::CircularReference(ref mut p)
                => *p = path,
            Error::Parse(_) | Error::Io(_) => {}
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

pub struct Parser {
    nodes: Vec<Node>,
    prelude: HashMap<String, usize>
}

impl Parser {
    pub fn new() -> Parser {
        Parser {
            nodes: vec![Node::new(
                NodeType::Object {
                    base: None,
                    children: HashMap::new()
                },
                0
            )],
            prelude: HashMap::new()
        }
    }

    pub fn parse_file<P: AsRef<Path>>(&mut self, path: P) -> Result<(), Error> {
        self.parse_file_in(path, 0, None)
    }

    fn parse_file_in<P: AsRef<Path>>(&mut self, path: P, root: usize, new_root: Option<ast::Path>) -> Result<(), Error> {
        let mut source = String::new();
        let mut file = try!(File::open(&path));
        try!(file.read_to_string(&mut source));
        let new_root = if let Some(new_root) = new_root {
            try!(self.expect(root, root, new_root))
        } else {
            0
        };
        self.parse(path, &source, new_root)
    }

    pub fn parse_string(&mut self, source: &str) -> Result<(), Error> {
        self.parse(".", source, 0)
    }

    pub fn root(&self) -> Entry {
        Entry::root_of(self)
    }

    fn parse<P: AsRef<Path>>(&mut self, path: P, source: &str, root: usize) -> Result<(), Error> {
        let statements = try!(parser::parse(source));

        for statement in statements {
            /*let parser::Span {
                item: statement,
                ..
            } = statement;*/

            match statement {
                ast::Statement::Include(file, new_root) => {
                    let path = path.as_ref().join(file);
                    try!(self.parse_file_in(path, root, new_root))
                },
                ast::Statement::Assign(path, value) => try!(self.assign(root, path, value))
            }
        }

        Ok(())
    }

    fn assign(&mut self, root: usize, path: ast::Path, value: ast::Value) -> Result<(), Error> {
        let mut stack = Stack::new(root);
        self.assign_with_stack(&mut stack, Some(path), value)
    }

    fn assign_with_stack(&mut self, stack: &mut Stack, path: Option<ast::Path>, value: ast::Value) -> Result<(), Error> {
        let (path, mut stack) = if let Some(path) = path {
            let ast::Path {
                path_type,
                path
            } = path;

            let stack = if let ast::PathType::Global = path_type {
                MaybeOwnedMut::Owned(Stack::new(stack.root))
            } else {
                MaybeOwnedMut::Borrowed(stack)
            };

            (Some(path), stack)
        } else {
            (None, MaybeOwnedMut::Borrowed(stack))
        };

        let current_root = stack.current_id();
        
        let should_pop = if let Some(path) = path {
            try!(self.push_stack_section(&mut stack, path));
            true
        } else {
            false
        };

        let target_id = stack.top().unwrap().id;

        match value {
            ast::Value::Object(ast::Object::New(children)) => {
                self.nodes[target_id].ty = NodeType::Object {
                    base: None,
                    children: HashMap::new()
                };

                for (path, value) in children {
                    try!(self.assign_with_stack(&mut stack, Some(path), value));
                }
            },
            ast::Value::Object(ast::Object::Extension(base, extension)) => {
                let base_id = try!(self.expect(stack.root, current_root, base));

                self.nodes[target_id].ty = NodeType::Object {
                    base: Some(base_id),
                    children: HashMap::new()
                };

                match extension {
                    Some(ast::ExtensionChanges::BlockStyle(changes)) => {
                        if !self.infer_object(base_id) {
                            return Err(Error::NotAnObject(stack.to_path()));
                        }

                        for (path, value) in changes {
                            try!(self.assign_with_stack(&mut stack, Some(path), value));
                        }
                    },
                    Some(ast::ExtensionChanges::FunctionStyle(changes)) => {
                        if !self.infer_object(base_id) {
                            return Err(Error::NotAnObject(stack.to_path()));
                        }

                        for value in changes {
                            unimplemented!();
                        }
                    },
                    None => self.nodes[target_id].ty = NodeType::Link(base_id)
                }
            },
            ast::Value::List(l) => {
                let mut items = vec![];

                for (i, item) in l.into_iter().enumerate() {
                    let id = self.add_node(Node::new(
                        NodeType::Unknown,
                        target_id
                    ));
                    stack.entries.push(vec![StackEntry {
                        id: id,
                        selection: Selection::Index(i)
                    }]);
                    try!(self.assign_with_stack(&mut stack, None, item));
                    stack.pop_section();
                    items.push(id);
                };

                self.nodes[target_id].ty = NodeType::List(items);
            },
            ast::Value::String(s) => self.nodes[target_id].ty = NodeType::Value(Value::String(s)),
            ast::Value::Number(n) => self.nodes[target_id].ty = NodeType::Value(Value::Number(n))
        }

        if should_pop {
            stack.pop_section();
        }

        Ok(())
    }

    fn push_stack_section(&mut self, stack: &mut Stack, path: Vec<String>) -> Result<(), Error> {
        let mut current_root = stack.current_id();
        let mut section: Vec<StackEntry> = vec![];

        for ident in path {
            let maybe_id = if let &mut NodeType::Object { ref mut children, .. } = &mut self.nodes[current_root].ty {
                children.get_mut(&ident).map(|c| {
                    c.real = true;
                    c.id
                })
            } else {
                return Err(Error::NotAnObject(stack.to_path()));
            };

            let id = if let Some(id) = maybe_id {
                id
            } else {
                let new_id = self.add_node(Node::new(
                    NodeType::Object {
                        base: None,
                        children: HashMap::new()
                    },
                    current_root
                ));

                let res = self.push_child_to(current_root, ident.clone(), NodeChild {
                    id: new_id,
                    real: true
                });

                if let Err(mut e) = res {
                    let mut path = stack.to_path();
                    for entry in section {
                        path.push(entry.selection);
                    }
                    e.set_path(path);
                    return Err(e)
                }

                new_id
            };

            section.push(StackEntry {
                id: id,
                selection: Selection::Ident(ident)
            });

            current_root = id;
        }

        stack.entries.push(section);
        Ok(())
    }

    fn expect(&mut self, root_id: usize, current_id: usize, path: ast::Path) -> Result<usize, Error> {
        let mut current_root = if let ast::PathType::Global = path.path_type {
            root_id
        } else {
            current_id
        };

        let mut selection_path = vec![];

        for (i, ident) in path.path.into_iter().enumerate() {
            selection_path.push(Selection::Ident(ident.clone()));

            let maybe_id = if let NodeType::Object { ref children, .. } = self.nodes[current_root].ty {
                children.get(&ident).map(|c| c.id).or_else(|| if i == 0 {
                    self.prelude.get(&ident).cloned()
                } else {
                    None
                })
            } else {
                return Err(Error::NotAnObject(selection_path));
            };

            let id = if let Some(id) = maybe_id {
                id
            } else {
                let new_id = self.add_node(Node::new(
                    NodeType::Unknown,
                    current_root
                ));

                let res = self.push_child_to(current_root, ident, NodeChild {
                    id: new_id,
                    real: false
                });

                if let Err(mut e) = res {
                    e.set_path(selection_path);
                    return Err(e);
                }

                new_id
            };

            current_root = id;
        }

        Ok(current_root)
    }

    fn add_node(&mut self, node: Node) -> usize {
        self.nodes.push(node);
        self.nodes.len() - 1
    }

    fn push_child_to(&mut self, id: usize, ident: String, child: NodeChild) -> Result<(), Error> {
        let template = match &mut self.nodes[id].ty {
            ty @ &mut NodeType::Unknown => {
                let mut children = HashMap::new();
                children.insert(ident, child);

                *ty = NodeType::Object {
                    base: None,
                    children: children
                };

                return Ok(());
            },
            &mut NodeType::Link(template) => template,
            &mut NodeType::Object { ref mut children, .. } => {
                children.insert(ident, child);
                return Ok(());
            },
            _ => return Err(Error::NotAnObject(vec![]))
        };

        try!(self.verify_object_link(template));

        let mut children = HashMap::new();
        children.insert(ident, child);

        self.nodes[id].ty = NodeType::Object {
            base: Some(template),
            children: children
        };

        Ok(())
    }

    fn verify_object_link(&self, template: usize) -> Result<(), Error> {
        let mut current_template = template;
        loop {
            current_template = match self.nodes[current_template].ty {
                NodeType::Object { base: Some(t), .. } => t,
                NodeType::Object { base: None, .. } => return Ok(()),
                NodeType::Link(t) => t,
                _ => return Err(Error::NotAnObject(vec![]))
            };

            if template == current_template {
                return Err(Error::CircularReference(vec![]));
            }
        }
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
                    children: HashMap::new()
                };
                true
            },
            &mut NodeType::Object { .. } => true,
            _ => false
        }
    }
}

pub trait Decode: Any {}

impl<T: Any> Decode for T {}

struct Decoder<T: Decode>(Box<Fn(Entry) -> Option<T>>);

impl<T: Decode> Decoder<T> {
    fn new<F>(decode_fn: F) -> Decoder<T> where
        F: Fn(Entry) -> Option<T>,
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
        children: HashMap<String, NodeChild>
    },
    Value(Value),
    List(Vec<usize>)
}

#[derive(PartialEq, Debug)]
struct NodeChild {
    id: usize,
    real: bool
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

#[derive(PartialEq, Debug)]
pub enum Value {
    Number(Number),
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
pub enum Selection {
    Ident(String),
    Index(usize)
}

struct StackEntry {
    pub id: usize,
    pub selection: Selection
}

struct Stack {
    root: usize,
    entries: Vec<Vec<StackEntry>>
}

impl Stack {
    fn new(root: usize) -> Stack {
        Stack {
            root: root,
            entries: vec![]
        }
    }

    fn current_id(&self) -> usize {
        self.top().map_or(self.root, |e| e.id)
    }

    fn top(&self) -> Option<&StackEntry> {
        self.entries.last().and_then(|s| s.last())
    }

    fn pop_section(&mut self) {
        self.entries.pop();
    }

    fn to_path(&self) -> Vec<Selection> {
        self.entries.iter().flat_map(|v| v.iter()).map(|e| e.selection.clone()).collect()
    }
}


enum MaybeOwnedMut<'a, T: 'a> {
    Owned(T),
    Borrowed(&'a mut T)
}

impl<'a, T> Deref for MaybeOwnedMut<'a, T> {
    type Target = T;

    fn deref(&self) -> &T {
        match *self {
            MaybeOwnedMut::Owned(ref t) => t,
            MaybeOwnedMut::Borrowed(ref t)  => *t
        }
    }
}

impl<'a, T> DerefMut for MaybeOwnedMut<'a, T> {
    fn deref_mut(&mut self) -> &mut T {
        match *self {
            MaybeOwnedMut::Owned(ref mut t) => t,
            MaybeOwnedMut::Borrowed(ref mut t)  => *t
        }
    }
}


#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use anymap::any::Any;
    use Parser;
    use Prelude;
    use Node;
    use NodeChild;
    use NodeType;
    use entry::{Entry, FromEntry};

    macro_rules! assert_ok {
        ($e: expr) => (if let Err(e) = $e {
            const EXPR: &'static str = stringify!($e);
            panic!("{} failed with error: {:?}", EXPR, e);
        })
    }

    #[derive(PartialEq, Debug)]
    struct SingleItem<T>(T);

    #[derive(PartialEq, Debug)]
    struct SingleItem2<T>(T);

    impl<'a, T: FromEntry<'a>> FromEntry<'a> for SingleItem<T> {
        fn from_entry(entry: Entry<'a>) -> Option<SingleItem<T>> {
            entry.as_object()
                .and_then(|o| o.get("a"))
                .and_then(|e| T::from_entry(e))
                .map(|item| SingleItem(item))
        }
    }

    impl<'a, T: FromEntry<'a>> FromEntry<'a> for SingleItem2<T> {
        fn from_entry(entry: Entry<'a>) -> Option<SingleItem2<T>> {
            entry.as_object()
                .and_then(|o| o.get("a"))
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

    #[test]
    fn assign_object() {
        let mut cfg = Parser::new();
        cfg.parse_string("a = {}").unwrap();
        assert_eq!(cfg.nodes, vec![
            Node::new(
                NodeType::Object {
                    base: None,
                    children: vec![("a".into(), NodeChild {
                        id: 1,
                        real: true
                    })].into_iter().collect()
                },
                0
            ),
            Node::new(
                NodeType::Object {
                    base: None,
                    children: HashMap::new()
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
                    children: vec![("a".into(), NodeChild {
                        id: 1,
                        real: true
                    })].into_iter().collect()
                },
                0
            ),
            Node::new(
                NodeType::Object {
                    base: None,
                    children: vec![("b".into(), NodeChild {
                        id: 2,
                        real: true
                    })].into_iter().collect()
                },
                0
            ),
            Node::new(
                NodeType::Object {
                    base: None,
                    children: HashMap::new()
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
                    children: vec![("a".into(), NodeChild {
                        id: 1,
                        real: true
                    })].into_iter().collect()
                },
                0
            ),
            Node::new(
                NodeType::Object {
                    base: None,
                    children: vec![("b".into(), NodeChild {
                        id: 2,
                        real: true
                    })].into_iter().collect()
                },
                0
            ),
            Node::new(
                NodeType::Object {
                    base: None,
                    children: HashMap::new()
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
                    children: vec![("a".into(), NodeChild {
                        id: 1,
                        real: true
                    })].into_iter().collect()
                },
                0
            ),
            Node::new(
                NodeType::Object {
                    base: None,
                    children: HashMap::new()
                },
                0
            )
        ]);

        cfg.parse_string("a.b = {}").unwrap();
        assert_eq!(cfg.nodes, vec![
            Node::new(
                NodeType::Object {
                    base: None,
                    children: vec![("a".into(), NodeChild {
                        id: 1,
                        real: true
                    })].into_iter().collect()
                },
                0
            ),
            Node::new(
                NodeType::Object {
                    base: None,
                    children: vec![("b".into(), NodeChild {
                        id: 2,
                        real: true
                    })].into_iter().collect()
                },
                0
            ),
            Node::new(
                NodeType::Object {
                    base: None,
                    children: HashMap::new()
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
                    children: vec![("a".into(), NodeChild {
                        id: 1,
                        real: true
                    }),
                    ("b".into(), NodeChild {
                        id: 2,
                        real: true
                    })].into_iter().collect()
                },
                0
            ),
            Node::new(
                NodeType::Object {
                    base: None,
                    children: HashMap::new()
                },
                0
            ),
            Node::new(
                NodeType::Object {
                    base: Some(1),
                    children: vec![("c".into(), NodeChild {
                        id: 3,
                        real: true
                    })].into_iter().collect()
                },
                0
            ),
            Node::new(
                NodeType::Object {
                    base: None,
                    children: HashMap::new()
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
                    children: vec![("a".into(), NodeChild {
                        id: 1,
                        real: true
                    })].into_iter().collect()
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
        assert_eq!(Some(SingleItem(vec![1, 2, 3])), cfg.root().decode());
    }

    #[test]
    fn decode_float_list() {
        let mut cfg = Parser::new();
        assert_ok!(cfg.parse_string("a = [1.0, -2.4, 3.8]"));
        assert_eq!(Some(SingleItem(vec![1.0, -2.4, 3.8])), cfg.root().decode());
    }

    #[test]
    fn decode_string_list() {
        let mut cfg = Parser::new();
        assert_ok!(cfg.parse_string("a = [\"foo\", \"bar\"]"));
        assert_eq!(Some(SingleItem(vec!["foo", "bar"])), cfg.root().decode());
    }

    #[test]
    fn dynamic_decode_integer_list() {
        let mut cfg = with_prelude::<Vec<i32>>();
        assert_ok!(cfg.parse_string("b = o { a = [1, 2, 3] }"));
        let b = cfg.root().get("b");
        assert_eq!(Some(SingleItem(vec![1, 2, 3])), b.dynamic_decode());
        assert_eq!(Some(SingleItem2(vec![1, 2, 3])), b.dynamic_decode());
    }

    #[test]
    fn parse_relative_in_inner_root() {
        let mut cfg = Parser::new();
        assert_ok!(cfg.parse_string("a.b.c = {}"));
        let root = cfg.nodes.len() - 1;
        assert_ok!(cfg.parse(".", "d = 1", root));
        assert_eq!(Some(1), cfg.root().get("a").get("b").get("c").get("d").decode());
    }

    #[test]
    fn parse_absolute_in_inner_root() {
        let mut cfg = Parser::new();
        assert_ok!(cfg.parse_string("a.b.c = {}"));
        let root = cfg.nodes.len() - 1;
        assert_ok!(cfg.parse(".", "global.d = 1", root));
        assert_eq!(Some(1), cfg.root().get("a").get("b").get("c").get("d").decode());
    }

    #[test]
    fn link_in_inner_root() {
        let mut cfg = Parser::new();
        assert_ok!(cfg.parse_string("a.b.c = {} f = []"));
        let root = cfg.nodes.len() - 2;
        assert_ok!(cfg.parse(".", "d = f{}", root));
        assert_ok!(cfg.parse(".", "global.d = f{}", root));
        assert!(cfg.parse_string("d = f{}").is_err());
    }
}
