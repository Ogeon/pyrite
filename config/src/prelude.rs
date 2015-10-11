use std::collections::HashMap;

use Parser;
use Node;
use NodeType;
use NodeChild;
use Value;
use Decode;
use Decoder;

use entry::Entry;

pub struct Prelude(Parser);

impl Prelude {
    pub fn new() -> Prelude {
        Prelude(Parser::new())
    }

    pub fn object<'a>(&'a mut self, ident: String) -> Object<'a> {
        let id = self.0.add_node(Node::new(
            NodeType::Object {
                base: None,
                children: HashMap::new()
            },
            0
        ));
        self.0.prelude.insert(ident, id);

        Object {
            cfg: &mut self.0,
            id: id
        }
    }

    pub fn list<'a>(&'a mut self, ident: String) -> List<'a> {
        let id = self.0.add_node(Node::new(
            NodeType::List(vec![]),
            0
        ));
        self.0.prelude.insert(ident, id);

        List {
            cfg: &mut self.0,
            id: id
        }
    }

    pub fn value<'a, V: Into<Value>>(&'a mut self, ident: String, value: V) {
        let id = self.0.add_node(Node::new(
            NodeType::Value(value.into()),
            0
        ));
        self.0.prelude.insert(ident, id);
    }

    pub fn into_parser(self) -> Parser {
        self.0
    }
}

pub struct Object<'a> {
    cfg: &'a mut Parser,
    id: usize
}

impl<'a> Object<'a> {
    pub fn object<'b>(&'b mut self, ident: String) -> Object<'b> {
        let new_id = self.cfg.add_node(Node::new(
            NodeType::Object {
                base: None,
                children: HashMap::new()
            },
            self.id
        ));

        if let &mut NodeType::Object { ref mut children, .. }  = &mut self.cfg.nodes[self.id].ty {
            children.insert(ident, NodeChild {
                id: new_id,
                real: true
            });
        }

        Object {
            cfg: self.cfg,
            id: new_id
        }
    }

    pub fn list<'b>(&'b mut self, ident: String) -> List<'b> {
        let new_id = self.cfg.add_node(Node::new(
            NodeType::List(vec![]),
            self.id
        ));

        if let &mut NodeType::Object { ref mut children, .. }  = &mut self.cfg.nodes[self.id].ty {
            children.insert(ident, NodeChild {
                id: new_id,
                real: true
            });
        }

        List {
            cfg: self.cfg,
            id: new_id
        }
    }

    pub fn value<V: Into<Value>>(&mut self, ident: String, value: V) {
        let new_id = self.cfg.add_node(Node::new(
            NodeType::Value(value.into()),
            self.id
        ));
        if let &mut NodeType::Object { ref mut children, .. }  = &mut self.cfg.nodes[self.id].ty {
            children.insert(ident, NodeChild {
                id: new_id,
                real: true
            });
        }
    }

    pub fn add_decoder<T, F>(&mut self, decoder_fn: F) where
        T: Decode,
        F: Fn(Entry) -> Option<T>,
        F: 'static
    {
        self.cfg.nodes[self.id].decoder.insert(Decoder::new(decoder_fn));
    }
}


pub struct List<'a> {
    cfg: &'a mut Parser,
    id: usize
}

impl<'a> List<'a> {
    pub fn object<'b>(&'b mut self) -> Object<'b> {
        let new_id = self.cfg.add_node(Node::new(
            NodeType::Object {
                base: None,
                children: HashMap::new()
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

    pub fn value<V: Into<Value>>(&mut self, value: V) {
        let new_id = self.cfg.add_node(Node::new(
            NodeType::Value(value.into()),
            self.id
        ));
        if let &mut NodeType::List(ref mut items) = &mut self.cfg.nodes[self.id].ty {
            items.push(new_id);
        }
    }

    pub fn add_decoder<T, F>(&mut self, decoder_fn: F) where
        T: Decode,
        F: Fn(Entry) -> Option<T>,
        F: 'static
    {
        self.cfg.nodes[self.id].decoder.insert(Decoder::new(decoder_fn));
    }
}
