use std::{
    borrow::Cow,
    collections::{hash_map::Entry, HashMap},
    error::Error,
};

use super::tables::{TableExt, TableId, Tables};
use crate::light_source;

type Spectrum = Cow<'static, [(f32, f32)]>;

pub struct Spectra {
    spectra: Vec<Spectrum>,
}

impl Spectra {
    pub fn new() -> Self {
        Spectra {
            spectra: Vec::new(),
        }
    }

    pub fn get(&self, id: SpectrumId) -> &Spectrum {
        self.spectra.get(id.0).expect("missing spectrum")
    }

    fn insert(&mut self, spectrum: Spectrum) -> SpectrumId {
        let id = self.spectra.len();
        self.spectra.push(spectrum);
        SpectrumId(id)
    }
}

pub struct SpectrumLoader {
    spectra: Spectra,
    file_map: HashMap<TableId, SpectrumId>,
}

impl SpectrumLoader {
    pub fn new() -> Self {
        SpectrumLoader {
            spectra: Spectra::new(),
            file_map: HashMap::new(),
        }
    }

    pub fn insert(
        &mut self,
        table: rlua::Table<'_>,
        tables: &Tables,
    ) -> Result<SpectrumId, Box<dyn Error>> {
        let id = table.get_or_assign_id(tables)?;

        match self.file_map.entry(id) {
            Entry::Occupied(entry) => Ok(*entry.get()),
            Entry::Vacant(entry) => {
                let spectrum = rlua_serde::from_value(rlua::Value::Table(table))?;
                let id = self.spectra.insert(spectrum);
                entry.insert(id);
                Ok(id)
            }
        }
    }

    pub fn insert_static(&mut self, table: rlua::Table<'_>) -> Result<SpectrumId, Box<dyn Error>> {
        let id = table.get_id()?;

        match self.file_map.entry(id) {
            Entry::Occupied(entry) => Ok(*entry.get()),
            Entry::Vacant(entry) => {
                let name: String = table.get("name")?;
                let spectrum = match &*name {
                    "d65" => light_source::D65,
                    _ => return Err(format!("unknown builtin spectrum: {}", name).into()),
                };
                let id = self.spectra.insert(Cow::Borrowed(spectrum));
                entry.insert(id);
                Ok(id)
            }
        }
    }

    pub fn into_spectra(self) -> Spectra {
        self.spectra
    }
}

#[derive(Debug, Copy, Clone, Ord, PartialOrd, Eq, PartialEq, Hash)]
#[repr(transparent)]
pub struct SpectrumId(usize);
