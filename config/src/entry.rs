use std::collections::HashMap;

use Value;
use NodeType;
use NodeChild;
use Parser;
use Number;
use Decoder;
use Decode;

#[derive(Clone)]
pub struct Entry<'a> {
    cfg: &'a Parser,
    id: usize
}

impl<'a> Entry<'a> {
    pub fn root_of(cfg: &Parser) -> Entry {
        Entry {
            cfg: cfg,
            id: 0
        }
    }

    pub fn as_value(&self) -> Option<&'a Value> {
        if let NodeType::Value(ref val) = self.cfg.get_concrete_node(self.id).ty {
            Some(val)
        } else {
            None
        }
    }

    pub fn as_list(&self) -> Option<List<'a>> {
        if let NodeType::List(ref list) = self.cfg.get_concrete_node(self.id).ty {
            Some(List {
                cfg: self.cfg,
                list: list
            })
        } else {
            None
        }
    }

    pub fn as_object(&self) -> Option<Object<'a>> {
        if let NodeType::Object { ref base, ref children} = self.cfg.get_concrete_node(self.id).ty {
            Some(Object {
                cfg: self.cfg,
                template: base.clone(),
                children: children
            })
        } else {
            None
        }
    }

    pub fn get(&self, key: &str) -> Entry<'a> {
        self.as_object().expect("the entry is not an object").get(key).expect("invalid key")
    }

    pub fn index(&self, index: usize) -> Entry<'a> {
        self.as_list().expect("the entry is not a list").get(index).expect("invalid index")
    }

    pub fn decode<T: FromEntry<'a>>(&self) -> Option<T> {
        T::from_entry(self.clone())
    }

    pub fn dynamic_decode<T: Decode>(&self) -> Option<T> {
        self.cfg.get_decoder(self.id).and_then(|&Decoder(ref decoder)| decoder(self.clone()))
    }
}

pub trait FromEntry<'a> {
    fn from_entry(entry: Entry<'a>) -> Option<Self>;
}

impl<'a, T: FromEntry<'a>> FromEntry<'a> for Vec<T> {
    fn from_entry(entry: Entry<'a>) -> Option<Vec<T>> {
        if let Some(list) = entry.as_list() {
            let mut v = vec![];
            for entry in list {
                if let Some(item) = T::from_entry(entry) {
                    v.push(item);
                } else {
                    return None;
                }
            }

            Some(v)
        } else {
            None
        }
    }
}

impl<'a> FromEntry<'a> for &'a str {
    fn from_entry(entry: Entry<'a>) -> Option<&'a str> {
        entry.as_value().and_then(|v| if let &Value::String(ref s) = v {
            Some(&**s)
        } else {
            None
        })
    }
}

impl<'a> FromEntry<'a> for String {
    fn from_entry(entry: Entry<'a>) -> Option<String> {
        entry.as_value().and_then(|v| if let &Value::String(ref s) = v {
            Some(s.clone())
        } else {
            None
        })
    }
}

macro_rules! int_from_entry {
    ($($ty: ty),+) => ($(
        impl<'a> FromEntry<'a> for $ty {
            fn from_entry(entry: Entry<'a>) -> Option<$ty> {
                entry.as_value().and_then(|v| if let &Value::Number(Number::Integer(i)) = v {
                    Some(i as $ty)
                } else {
                    None
                })
            }
        }
    )+)
}

macro_rules! float_from_entry {
    ($($ty: ty),+) => ($(
        impl<'a> FromEntry<'a> for $ty {
            fn from_entry(entry: Entry<'a>) -> Option<$ty> {
                entry.as_value().and_then(|v| match *v {
                    Value::Number(Number::Float(f)) => Some(f as $ty),
                    Value::Number(Number::Integer(i)) => Some(i as $ty),
                    _ => None,
                })
            }
        }
    )+)
}

int_from_entry!(u8, u16, u32, u64, i8, i16, i32, i64);
float_from_entry!(f32, f64);

#[derive(Clone)]
pub struct List<'a> {
    cfg: &'a Parser,
    list: &'a [usize]
}

impl<'a> List<'a> {
    pub fn get(&self, index: usize) -> Option<Entry<'a>> {
        self.list.get(index).map(|&id| Entry {
            cfg: self.cfg,
            id: id
        })
    }

    pub fn iter(&self) -> IntoIter {
        IntoIter {
            cfg: self.cfg,
            iter: self.list.iter()
        }
    }
}

impl<'a> IntoIterator for List<'a> {
    type IntoIter = IntoIter<'a>;
    type Item = Entry<'a>;

    fn into_iter(self) -> IntoIter<'a> {
        IntoIter {
            cfg: self.cfg,
            iter: self.list.iter()
        }
    }
}

impl<'a> IntoIterator for &'a List<'a> {
    type IntoIter = IntoIter<'a>;
    type Item = Entry<'a>;

    fn into_iter(self) -> IntoIter<'a> {
        IntoIter {
            cfg: self.cfg,
            iter: self.list.iter()
        }
    }
}

pub struct IntoIter<'a> {
    cfg: &'a Parser,
    iter: ::std::slice::Iter<'a, usize>
}

impl<'a> Iterator for IntoIter<'a> {
    type Item = Entry<'a>;

    fn next(&mut self) -> Option<Entry<'a>> {
        self.iter.next().map(|&id| Entry {
            cfg: self.cfg,
            id: id
        })
    }
}

#[derive(Clone)]
pub struct Object<'a> {
    cfg: &'a Parser,
    template: Option<usize>,
    children: &'a HashMap<String, NodeChild>
}

impl<'a> Object<'a> {
    pub fn get(&self, key: &str) -> Option<Entry<'a>> {
        let mut children = self.children;
        let mut template = self.template.clone();

        loop {
            if let Some(child) = children.get(key) {
                return Some(Entry {
                    cfg: self.cfg,
                    id: child.id
                })
            } else {
                if let Some(t) = template {
                    if let NodeType::Object { base: ref t, children: ref c } = self.cfg.get_concrete_node(t).ty {
                        children = c;
                        template = t.clone();
                    } else {
                        return None
                    }
                } else {
                    return None
                }
            }
        }
    }
}
