mod parser;
mod lexer;

use std::path::Path;
use std::fs::File;
use std::io::Read;
use std::ops::{Deref, DerefMut};
use std::collections::HashMap;

pub use parser::Number;

#[derive(Debug)]
pub enum Error {
    Parse(parser::Error),
    Io(std::io::Error),
    NotAnObject(usize)
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

pub struct ConfigPrelude(ConfigParser);

impl ConfigPrelude {
    pub fn new() -> ConfigPrelude {
        ConfigPrelude(ConfigParser::new())
    }

    pub fn add_prelude_object<'a>(&'a mut self, ident: String) -> PreludeObject<'a> {
        let id = self.0.add_node(Node::Object {
            base: None,
            children: vec![]
        });
        self.0.prelude.insert(ident, id);

        PreludeObject {
            cfg: &mut self.0,
            id: id
        }
    }

    pub fn add_prelude_list<'a>(&'a mut self, ident: String) -> PreludeList<'a> {
        let id = self.0.add_node(Node::List(vec![]));
        self.0.prelude.insert(ident, id);

        PreludeList {
            cfg: &mut self.0,
            id: id
        }
    }

    pub fn add_prelude_value<'a, V: Into<Value>>(&'a mut self, ident: String, value: V) {
        let id = self.0.add_node(Node::Value(value.into()));
        self.0.prelude.insert(ident, id);
    }

    pub fn into_parser(self) -> ConfigParser {
        self.0
    }
}

pub struct ConfigParser {
    nodes: Vec<Node>,
    prelude: HashMap<String, usize>
}

impl ConfigParser {
    pub fn new() -> ConfigParser {
        ConfigParser {
            nodes: vec![Node::Object {
                base: None,
                children: vec![]
            }],
            prelude: HashMap::new()
        }
    }

    pub fn parse_file<P: AsRef<Path>>(&mut self, path: P) -> Result<(), Error> {
        let mut source = String::new();
        let mut file = try!(File::open(&path));
        try!(file.read_to_string(&mut source));
        self.parse(path, &source)
    }

    pub fn parse_string(&mut self, source: &str) -> Result<(), Error> {
        self.parse(".", source)
    }

    fn parse<P: AsRef<Path>>(&mut self, path: P, source: &str) -> Result<(), Error> {
        let statements = try!(parser::parse(source.chars()));

        for statement in statements {
            let parser::Span {
                item: statement,
                ..
            } = statement;

            match statement {
                parser::Statement::Include(p) => {
                    let path = path.as_ref().join(p);
                    try!(self.parse_file(path))
                },
                parser::Statement::Assign(path, value) => try!(self.assign(path, value))
            }
        }

        Ok(())
    }

    fn assign(&mut self, path: parser::Path, value: parser::Value) -> Result<(), Error> {
        let mut stack = Stack::new();
        self.assign_with_stack(&mut stack, path, value)
    }

    fn assign_with_stack(&mut self, stack: &mut Stack, path: parser::Path, value: parser::Value) -> Result<(), Error> {
        let parser::Path {
            path_type,
            path
        } = path;

        let mut stack = if let parser::PathType::Global = path_type {
            MaybeOwnedMut::Owned(Stack::new())
        } else {
            MaybeOwnedMut::Borrowed(stack)
        };

        let current_root = stack.top().map_or(0, |e| e.id);
        
        try!(self.push_stack_section(&mut stack, path));

        let target_id = stack.top().unwrap().id;

        match value {
            parser::Value::Object(parser::Object::New(children)) => {
                self.nodes[target_id] = Node::Object {
                    base: None,
                    children: vec![]
                };

                for (path, value) in children {
                    try!(self.assign_with_stack(&mut stack, path, value));
                }
            },
            parser::Value::Object(parser::Object::Extension(base, extension)) => {
                let base_id = try!(self.expect(current_root, base));

                self.nodes[target_id] = Node::Object {
                    base: Some(base_id),
                    children: vec![]
                };

                match extension {
                    Some(parser::ExtensionChanges::BlockStyle(changes)) => {
                        for (path, value) in changes {
                            try!(self.assign_with_stack(&mut stack, path, value));
                        }
                    },
                    Some(parser::ExtensionChanges::FunctionStyle(changes)) => {
                        for value in changes {
                            unimplemented!();
                        }
                    },
                    None => {}
                }
            },
            parser::Value::List(l) => {
                unimplemented!();
            },
            parser::Value::String(s) => self.nodes[target_id] = Node::Value(Value::String(s)),
            parser::Value::Number(n) => self.nodes[target_id] = Node::Value(Value::Number(n))
        }

        stack.pop_section();

        Ok(())
    }

    fn push_stack_section(&mut self, stack: &mut Stack, path: Vec<String>) -> Result<(), Error> {
        let mut current_root = stack.top().map_or(0, |e| e.id);
        let mut section = vec![];

        for ident in path {
            let maybe_id = if let &mut Node::Object { ref mut children, .. } = &mut self.nodes[current_root] {
                children.iter_mut().find(|c| c.ident == ident).map(|c| {
                    c.real = true;
                    c.id
                })
            } else {
                return Err(Error::NotAnObject(current_root));
            };

            let id = maybe_id.unwrap_or_else(|| {
                let new_id = self.add_node(Node::Object {
                    base: None,
                    children: vec![]
                });

                if let &mut Node::Object { ref mut children, .. } = &mut self.nodes[current_root] {
                    children.push(NodeChild {
                        id: new_id,
                        ident: ident.clone(),
                        real: true
                    });
                }

                new_id
            });

            section.push(StackEntry {
                id: id,
                name: ident
            });

            current_root = id;
        }

        stack.entries.push(section);
        Ok(())
    }

    fn expect(&mut self, root_id: usize, path: parser::Path) -> Result<usize, Error> {
        let mut current_root = if let parser::PathType::Global = path.path_type {
            0
        } else {
            root_id
        };

        for (i, ident) in path.path.into_iter().enumerate() {
            let maybe_id = if let Node::Object { ref children, .. } = self.nodes[current_root] {
                children.iter().find(|c| c.ident == ident).map(|c| c.id).or_else(|| if i == 0 {
                    self.prelude.get(&ident).cloned()
                } else {
                    None
                })
            } else {
                return Err(Error::NotAnObject(current_root));
            };

            let id = if let Some(id) = maybe_id {
                id
            } else {
                let new_id = self.add_node(Node::Object {
                    base: None,
                    children: vec![]
                });

                if let &mut Node::Object { ref mut children, .. } = &mut self.nodes[current_root] {
                    children.push(NodeChild {
                        id: new_id,
                        ident: ident,
                        real: false
                    });
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
}


pub struct PreludeObject<'a> {
    cfg: &'a mut ConfigParser,
    id: usize
}

impl<'a> PreludeObject<'a> {
    pub fn insert_object<'b>(&'b mut self, ident: String) -> PreludeObject<'b> {
        let new_id = self.cfg.add_node(Node::Object {
            base: None,
            children: vec![]
        });

        if let &mut Node::Object { ref mut children, .. }  = &mut self.cfg.nodes[self.id] {
            children.push(NodeChild {
                id: new_id,
                ident: ident,
                real: true
            });
        }

        PreludeObject {
            cfg: self.cfg,
            id: new_id
        }
    }

    pub fn insert_list<'b>(&'b mut self, ident: String) -> PreludeList<'b> {
        let new_id = self.cfg.add_node(Node::List(vec![]));

        if let &mut Node::Object { ref mut children, .. }  = &mut self.cfg.nodes[self.id] {
            children.push(NodeChild {
                id: new_id,
                ident: ident,
                real: true
            });
        }

        PreludeList {
            cfg: self.cfg,
            id: new_id
        }
    }

    pub fn insert_value<V: Into<Value>>(&mut self, ident: String, value: V) {
        let new_id = self.cfg.add_node(Node::Value(value.into()));
        if let &mut Node::Object { ref mut children, .. }  = &mut self.cfg.nodes[self.id] {
            children.push(NodeChild {
                id: new_id,
                ident: ident,
                real: true
            });
        }
    }
}


pub struct PreludeList<'a> {
    cfg: &'a mut ConfigParser,
    id: usize
}

impl<'a> PreludeList<'a> {
    pub fn push_object<'b>(&'b mut self) -> PreludeObject<'b> {
        let new_id = self.cfg.add_node(Node::Object {
            base: None,
            children: vec![]
        });

        if let &mut Node::List(ref mut items) = &mut self.cfg.nodes[self.id] {
            items.push(new_id);
        }

        PreludeObject {
            cfg: self.cfg,
            id: new_id
        }
    }

    pub fn push_list<'b>(&'b mut self) -> PreludeList<'b> {
        let new_id = self.cfg.add_node(Node::List(vec![]));

        if let &mut Node::List(ref mut items) = &mut self.cfg.nodes[self.id] {
            items.push(new_id);
        }

        PreludeList {
            cfg: self.cfg,
            id: new_id
        }
    }

    pub fn push_value<V: Into<Value>>(&mut self, value: V) {
        let new_id = self.cfg.add_node(Node::Value(value.into()));
        if let &mut Node::List(ref mut items) = &mut self.cfg.nodes[self.id] {
            items.push(new_id);
        }
    }
}

#[derive(PartialEq, Debug)]
enum Node {
    Object {
        base: Option<usize>,
        children: Vec<NodeChild>
    },
    Value(Value),
    List(Vec<usize>)
}

#[derive(PartialEq, Debug)]
struct NodeChild {
    id: usize,
    ident: String,
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

struct StackEntry {
    pub id: usize,
    pub name: String
}

struct Stack {
    entries: Vec<Vec<StackEntry>>
}

impl Stack {
    fn new() -> Stack {
        Stack {
            entries: vec![]
        }
    }

    fn top(&self) -> Option<&StackEntry> {
        self.entries.last().and_then(|s| s.last())
    }

    fn pop_section(&mut self) {
        self.entries.pop();
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
    use ConfigParser;
    use Node;
    use NodeChild;

    #[test]
    fn assign_object() {
        let mut cfg = ConfigParser::new();
        cfg.parse_string("a = {}").unwrap();
        assert_eq!(cfg.nodes, vec![
            Node::Object {
                base: None,
                children: vec![NodeChild {
                    id: 1,
                    ident: "a".into(),
                    real: true
                }]
            },
            Node::Object {
                base: None,
                children: vec![]
            }
        ]);
    }

    #[test]
    fn assign_deep_object() {
        let mut cfg = ConfigParser::new();
        cfg.parse_string("a.b = {}").unwrap();
        assert_eq!(cfg.nodes, vec![
            Node::Object {
                base: None,
                children: vec![NodeChild {
                    id: 1,
                    ident: "a".into(),
                    real: true
                }]
            },
            Node::Object {
                base: None,
                children: vec![NodeChild {
                    id: 2,
                    ident: "b".into(),
                    real: true
                }]
            },
            Node::Object {
                base: None,
                children: vec![]
            }
        ]);
    }

    #[test]
    fn assign_in_object() {
        let mut cfg = ConfigParser::new();
        cfg.parse_string("a = { b = {} }").unwrap();
        assert_eq!(cfg.nodes, vec![
            Node::Object {
                base: None,
                children: vec![NodeChild {
                    id: 1,
                    ident: "a".into(),
                    real: true
                }]
            },
            Node::Object {
                base: None,
                children: vec![NodeChild {
                    id: 2,
                    ident: "b".into(),
                    real: true
                }]
            },
            Node::Object {
                base: None,
                children: vec![]
            }
        ]);
    }

    #[test]
    fn assign_to_object() {
        let mut cfg = ConfigParser::new();
        cfg.parse_string("a = {}").unwrap();
        assert_eq!(cfg.nodes, vec![
            Node::Object {
                base: None,
                children: vec![NodeChild {
                    id: 1,
                    ident: "a".into(),
                    real: true
                }]
            },
            Node::Object {
                base: None,
                children: vec![]
            }
        ]);

        cfg.parse_string("a.b = {}").unwrap();
        assert_eq!(cfg.nodes, vec![
            Node::Object {
                base: None,
                children: vec![NodeChild {
                    id: 1,
                    ident: "a".into(),
                    real: true
                }]
            },
            Node::Object {
                base: None,
                children: vec![NodeChild {
                    id: 2,
                    ident: "b".into(),
                    real: true
                }]
            },
            Node::Object {
                base: None,
                children: vec![]
            }
        ]);
    }

    #[test]
    fn extend_block_style() {
        let mut cfg = ConfigParser::new();
        cfg.parse_string("a = {} b = a { c = {} }").unwrap();
        assert_eq!(cfg.nodes, vec![
            Node::Object {
                base: None,
                children: vec![NodeChild {
                    id: 1,
                    ident: "a".into(),
                    real: true
                },
                NodeChild {
                    id: 2,
                    ident: "b".into(),
                    real: true
                }]
            },
            Node::Object {
                base: None,
                children: vec![]
            },
            Node::Object {
                base: Some(1),
                children: vec![NodeChild {
                    id: 3,
                    ident: "c".into(),
                    real: true
                }]
            },
            Node::Object {
                base: None,
                children: vec![]
            }
        ]);
    }
}
