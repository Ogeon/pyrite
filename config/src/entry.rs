//!Tools for traversal and decoding of the parsed configuration.

use std::collections::HashMap;

use Decode;
use Decoder;
use NodeType;
use Number;
use Parser;
use Value;

///An entry in a parsed configuration.
#[derive(Clone)]
pub struct Entry<'a> {
    cfg: &'a Parser,
    id: usize,
}

impl<'a> Entry<'a> {
    ///Get the root entry from a particular parser.
    pub fn root_of(cfg: &Parser) -> Entry {
        Entry { cfg: cfg, id: 0 }
    }

    ///Use this entry as a primitive value, if possible.
    pub fn as_value(&self) -> Option<&'a Value> {
        if let NodeType::Value(ref val) = self.cfg.get_concrete_node(self.id).ty {
            Some(val)
        } else {
            None
        }
    }

    ///Use this entry as a list, if possible.
    pub fn as_list(&self) -> Option<List<'a>> {
        if let NodeType::List(ref list) = self.cfg.get_concrete_node(self.id).ty {
            Some(List {
                cfg: self.cfg,
                list: list,
            })
        } else {
            None
        }
    }

    ///Use this entry as an object, if possible.
    pub fn as_object(&self) -> Option<Object<'a>> {
        if let NodeType::Object {
            ref base,
            ref children,
            ..
        } = self.cfg.get_concrete_node(self.id).ty
        {
            Some(Object {
                cfg: self.cfg,
                template: base.clone(),
                children: children,
            })
        } else {
            None
        }
    }

    ///Assume that this is an object and get one of its entries.
    pub fn get(&self, key: &str) -> Entry<'a> {
        self.as_object()
            .expect("the entry is not an object")
            .get(key)
            .expect("invalid key")
    }

    ///Assume that this is a list and get one of its elements.
    pub fn index(&self, index: usize) -> Entry<'a> {
        self.as_list()
            .expect("the entry is not a list")
            .get(index)
            .expect("invalid index")
    }

    ///Try to decode this entry.
    pub fn decode<T: FromEntry<'a>>(&self) -> Result<T, String> {
        T::from_entry(self.clone())
    }

    ///Try to dynamically decode this entry.
    pub fn dynamic_decode<T: Decode>(&self) -> Result<T, String> {
        self.cfg
            .get_decoder(self.id)
            .ok_or("could not decode dynamically".into())
            .and_then(|&Decoder(ref decoder)| decoder(self.clone()))
    }
}

///A trait for types that can be decoded statically.
pub trait FromEntry<'a>: Sized {
    ///Try to decode this type from an entry.
    fn from_entry(entry: Entry<'a>) -> Result<Self, String>;
}

impl<'a, T: FromEntry<'a>> FromEntry<'a> for Vec<T> {
    fn from_entry(entry: Entry<'a>) -> Result<Vec<T>, String> {
        if let Some(list) = entry.as_list() {
            list.into_iter().map(|e| e.decode()).collect()
        } else {
            Err("expected a list".into())
        }
    }
}

impl<'a> FromEntry<'a> for &'a str {
    fn from_entry(entry: Entry<'a>) -> Result<&'a str, String> {
        entry
            .as_value()
            .ok_or("expected a value".into())
            .and_then(|v| {
                if let &Value::String(ref s) = v {
                    Ok(&**s)
                } else {
                    Err("expected a string".into())
                }
            })
    }
}

impl<'a> FromEntry<'a> for String {
    fn from_entry(entry: Entry<'a>) -> Result<String, String> {
        entry
            .as_value()
            .ok_or("expected a value".into())
            .and_then(|v| {
                if let &Value::String(ref s) = v {
                    Ok(s.clone())
                } else {
                    Err("expected a string".into())
                }
            })
    }
}

macro_rules! int_from_entry {
    ($($ty: ty),+) => ($(
        impl<'a> FromEntry<'a> for $ty {
            fn from_entry(entry: Entry<'a>) -> Result<$ty, String> {
                entry.as_value().ok_or("expected a value".into()).and_then(|v| if let &Value::Number(Number::Integer(i)) = v {
                    Ok(i as $ty)
                } else {
                    Err("expected an integer".into())
                })
            }
        }
    )+)
}

macro_rules! float_from_entry {
    ($($ty: ty),+) => ($(
        impl<'a> FromEntry<'a> for $ty {
            fn from_entry(entry: Entry<'a>) -> Result<$ty, String> {
                entry.as_value().ok_or("expected a value".into()).and_then(|v| match *v {
                    Value::Number(Number::Float(f)) => Ok(f as $ty),
                    Value::Number(Number::Integer(i)) => Ok(i as $ty),
                    _ => Err("expected a number".into()),
                })
            }
        }
    )+)
}

macro_rules! tuple_from_entry {
    ($first: ident $(,$types: ident)+) => (
        impl<'a, $first $(,$types)*> FromEntry<'a> for ($first $(,$types)*) where $first: FromEntry<'a> $(,$types: FromEntry<'a>)* {
            fn from_entry(entry: Entry<'a>) -> Result<Self, String> {
                entry.as_list().ok_or("expected a list".into()).and_then(|list| {
                    let len = list.len();
                    let mut iter = list.into_iter();
                    let result = ({
                            let i = iter.next().ok_or("too few items".into()).and_then(|e| $first::from_entry(e));
                            try!(i)
                        }
                        $(, {
                            let i = iter.next().ok_or("too few items".into()).and_then(|e| $types::from_entry(e));
                            try!(i)
                        })*
                    );

                    let rest = iter.count();

                    if rest == 0 {
                        Ok(result)
                    } else {
                        Err(format!("expected exactly {} items", len - rest))
                    }
                })
            }
        }

        tuple_from_entry!($($types),*);
    );
    ($ty: ident) => (
        impl<'a, $ty: FromEntry<'a>> FromEntry<'a> for ($ty,) {
            fn from_entry(entry: Entry<'a>) -> Result<Self, String> {
                entry.as_list().ok_or("expected a list".into()).and_then(|list| {
                    if list.len() == 1 {
                        Ok(({
                                try!($ty::from_entry(list.get(0).unwrap()))
                            },
                        ))
                    } else {
                        Err("expected exactly one item".into())
                    }
                })
            }
        }
    );
}

int_from_entry!(u8, u16, u32, u64, i8, i16, i32, i64, isize, usize);
float_from_entry!(f32, f64);
tuple_from_entry!(A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P);

#[derive(Clone)]
pub struct List<'a> {
    cfg: &'a Parser,
    list: &'a [usize],
}

///A list in a parsed configuration.
impl<'a> List<'a> {
    ///Get an element from the list.
    pub fn get(&self, index: usize) -> Option<Entry<'a>> {
        self.list.get(index).map(|&id| Entry {
            cfg: self.cfg,
            id: id,
        })
    }

    ///Iterate over the elements of the list.
    pub fn iter(&self) -> Items {
        Items {
            cfg: self.cfg,
            iter: self.list.iter(),
        }
    }

    ///The number of elements in the list.
    pub fn len(&self) -> usize {
        self.list.len()
    }
}

impl<'a> IntoIterator for List<'a> {
    type IntoIter = Items<'a>;
    type Item = Entry<'a>;

    fn into_iter(self) -> Items<'a> {
        Items {
            cfg: self.cfg,
            iter: self.list.iter(),
        }
    }
}

impl<'a> IntoIterator for &'a List<'a> {
    type IntoIter = Items<'a>;
    type Item = Entry<'a>;

    fn into_iter(self) -> Items<'a> {
        self.iter()
    }
}

///An iterator for list items.
pub struct Items<'a> {
    cfg: &'a Parser,
    iter: ::std::slice::Iter<'a, usize>,
}

impl<'a> Iterator for Items<'a> {
    type Item = Entry<'a>;

    fn next(&mut self) -> Option<Entry<'a>> {
        self.iter.next().map(|&id| Entry {
            cfg: self.cfg,
            id: id,
        })
    }
}

///An object in a parsed configuration.
#[derive(Clone)]
pub struct Object<'a> {
    cfg: &'a Parser,
    template: Option<usize>,
    children: &'a HashMap<String, usize>,
}

impl<'a> Object<'a> {
    ///Get an entry from the object.
    pub fn get(&self, key: &str) -> Option<Entry<'a>> {
        let mut children = self.children;
        let mut template = self.template.clone();

        loop {
            if let Some(&child) = children.get(key) {
                return Some(Entry {
                    cfg: self.cfg,
                    id: child,
                });
            } else {
                if let Some(t) = template {
                    if let NodeType::Object {
                        base: ref t,
                        children: ref c,
                        ..
                    } = self.cfg.get_concrete_node(t).ty
                    {
                        children = c;
                        template = t.clone();
                    } else {
                        return None;
                    }
                } else {
                    return None;
                }
            }
        }
    }

    ///Iterate over the entries in the object.
    pub fn iter(&self) -> Entries {
        Entries {
            cfg: self.cfg,
            iter: self.children.iter(),
        }
    }
}

impl<'a> IntoIterator for Object<'a> {
    type IntoIter = Entries<'a>;
    type Item = (&'a str, Entry<'a>);

    fn into_iter(self) -> Entries<'a> {
        Entries {
            cfg: self.cfg,
            iter: self.children.iter(),
        }
    }
}

impl<'a> IntoIterator for &'a Object<'a> {
    type IntoIter = Entries<'a>;
    type Item = (&'a str, Entry<'a>);

    fn into_iter(self) -> Entries<'a> {
        self.iter()
    }
}

///An iterator for object entries.
pub struct Entries<'a> {
    cfg: &'a Parser,
    iter: ::std::collections::hash_map::Iter<'a, String, usize>,
}

impl<'a> Iterator for Entries<'a> {
    type Item = (&'a str, Entry<'a>);

    fn next(&mut self) -> Option<(&'a str, Entry<'a>)> {
        self.iter.next().map(|(key, &entry)| {
            (
                &**key,
                Entry {
                    cfg: self.cfg,
                    id: entry,
                },
            )
        })
    }
}
