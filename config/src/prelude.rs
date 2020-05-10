//!Tools for defining decoders, argument lists and predefined values.

use std::collections::HashMap;

use crate::Parser;
use crate::Node;
use crate::NodeType;
use crate::Value;
use crate::Decode;
use crate::Decoder;

use crate::entry::Entry;

pub struct Prelude(Parser);

///The prelude initializer.
///
///It's used to define decoders, argument lists and values that can be
///referred to from within the configuration.
impl Prelude {
    ///Create an empty prelude.
    pub fn new() -> Prelude {
        Prelude(Parser::new())
    }

    ///Create or access an object in the prelude.
    pub fn object<'a>(&'a mut self, ident: String) -> Object<'a> {
        let maybe_id = self.0.prelude.get(&ident).map(|&id| id);

        let id = if let Some(id) = maybe_id {
            id
        } else {
            let id = self.0.add_node(Node::new(
                NodeType::Object {
                    base: None,
                    children: HashMap::new(),
                    arguments: vec![]
                },
                0
            ));
            self.0.prelude.insert(ident, id);
            id
        };

        Object {
            cfg: &mut self.0,
            id: id
        }
    }

    ///Create or access a list in the prelude.
    pub fn list<'a>(&'a mut self, ident: String) -> List<'a> {
        let maybe_id = self.0.prelude.get(&ident).map(|&id| id);
        let id = if let Some(id) = maybe_id {
            id
        } else {
            let id = self.0.add_node(Node::new(
                NodeType::List(vec![]),
                0
            ));
            self.0.prelude.insert(ident, id);
            id
        };

        List {
            cfg: &mut self.0,
            id: id
        }
    }

    ///Create a value in the prelude.
    pub fn value<'a, V: Into<Value>>(&'a mut self, ident: String, value: V) {
        let id = self.0.add_node(Node::new(
            NodeType::Value(value.into()),
            0
        ));
        self.0.prelude.insert(ident, id);
    }

    ///Turn the prelude into a parser. It's not possible to change the prelude
    ///after this point.
    pub fn into_parser(self) -> Parser {
        self.0
    }
}

///An object in the prelude.
pub struct Object<'a> {
    cfg: &'a mut Parser,
    id: usize
}

impl<'a> Object<'a> {
    ///Create or access an object inside this object.
    pub fn object<'b>(&'b mut self, ident: String) -> Object<'b> {
        let maybe_id = if let &mut NodeType::Object { ref mut children, .. } = &mut self.cfg.nodes[self.id].ty {
            children.get(&ident).map(|&node| node)
        } else {
            unreachable!()
        };

        let new_id = if let Some(id) = maybe_id {
            id
        } else {
            let id = self.cfg.add_node(Node::new(
                NodeType::Object {
                    base: None,
                    children: HashMap::new(),
                    arguments: vec![]
                },
                self.id
            ));

            if let &mut NodeType::Object { ref mut children, .. } = &mut self.cfg.nodes[self.id].ty {
                children.insert(ident, id);
            }

            id
        };

        Object {
            cfg: self.cfg,
            id: new_id
        }
    }

    ///Create or access a list inside this object.
    pub fn list<'b>(&'b mut self, ident: String) -> List<'b> {
        let maybe_id = if let &mut NodeType::Object { ref mut children, .. } = &mut self.cfg.nodes[self.id].ty {
            children.get(&ident).map(|&node| node)
        } else {
            unreachable!()
        };

        let new_id = if let Some(id) = maybe_id {
            id
        } else {
            let id = self.cfg.add_node(Node::new(
                NodeType::List(vec![]),
                self.id
            ));

            if let &mut NodeType::Object { ref mut children, .. } = &mut self.cfg.nodes[self.id].ty {
                children.insert(ident, id);
            }

            id
        };

        List {
            cfg: self.cfg,
            id: new_id
        }
    }

    ///Create a value inside this object.
    pub fn value<V: Into<Value>>(&mut self, ident: String, value: V) {
        let new_id = self.cfg.add_node(Node::new(
            NodeType::Value(value.into()),
            self.id
        ));
        if let &mut NodeType::Object { ref mut children, .. }  = &mut self.cfg.nodes[self.id].ty {
            children.insert(ident, new_id);
        }
    }

    ///Attach a decoder to this object. Multiple decoders are allowed, as long
    ///as they decodes to different types.
    pub fn add_decoder<T, F>(&mut self, decoder_fn: F) where
        T: Decode,
        F: Fn(Entry<'_>) -> Result<T, String>,
        F: 'static
    {
        self.cfg.nodes[self.id].decoder.insert(Decoder::new(decoder_fn));
    }

    ///Give this object an argument list that maps input to entry keys.
    pub fn arguments(&mut self, args: Vec<String>) {
        if let &mut NodeType::Object { ref mut arguments, .. }  = &mut self.cfg.nodes[self.id].ty {
            *arguments = args;
        }
    }
}

///A list in the prelude.
pub struct List<'a> {
    cfg: &'a mut Parser,
    id: usize
}

impl<'a> List<'a> {
    ///Add an object to the end of the list.
    pub fn object<'b>(&'b mut self) -> Object<'b> {
        let new_id = self.cfg.add_node(Node::new(
            NodeType::Object {
                base: None,
                children: HashMap::new(),
                arguments: vec![]
            },
            self.id
        ));

        if let &mut NodeType::List(ref mut items) = &mut self.cfg.nodes[self.id].ty {
            items.push(new_id);
        }

        Object {
            cfg: self.cfg,
            id: new_id
        }
    }

    ///Add another list to the end of this list.
    pub fn list<'b>(&'b mut self) -> List<'b> {
        let new_id = self.cfg.add_node(Node::new(
            NodeType::List(vec![]),
            self.id
        ));

        if let &mut NodeType::List(ref mut items) = &mut self.cfg.nodes[self.id].ty {
            items.push(new_id);
        }

        List {
            cfg: self.cfg,
            id: new_id
        }
    }

    ///Add a value to the end of the list.
    pub fn value<V: Into<Value>>(&mut self, value: V) {
        let new_id = self.cfg.add_node(Node::new(
            NodeType::Value(value.into()),
            self.id
        ));
        if let &mut NodeType::List(ref mut items) = &mut self.cfg.nodes[self.id].ty {
            items.push(new_id);
        }
    }

    ///Attach a decoder to this object. Multiple decoders are allowed, as long
    ///as they decodes to different types.
    pub fn add_decoder<T, F>(&mut self, decoder_fn: F) where
        T: Decode,
        F: Fn(Entry<'_>) -> Result<T, String>,
        F: 'static
    {
        self.cfg.nodes[self.id].decoder.insert(Decoder::new(decoder_fn));
    }
}
