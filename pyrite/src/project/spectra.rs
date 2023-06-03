use std::{
    borrow::Cow,
    collections::{hash_map::Entry, HashMap},
    error::Error,
    ops::{Add, Mul, Sub},
};

use typed_nodes::TableId;

use super::parse_context::ParseContext;
use crate::{light_source, math::utils::Interpolated};

#[derive(Clone, typed_nodes::FromLua)]
#[typed_nodes(tag = format)]
pub enum Spectrum<T: Clone + 'static> {
    Array {
        min: f32,
        max: f32,
        points: Cow<'static, [T]>,
    },
    Curve {
        points: Vec<(f32, T)>,
    },
}

impl<T> Spectrum<T>
where
    T: Clone + Default + Add<Output = T> + Sub<Output = T> + Mul<f32, Output = T> + 'static,
{
    pub fn get(&self, wavelength: f32) -> T {
        match self {
            Spectrum::Array { min, max, points } => {
                if points.is_empty() {
                    return Default::default();
                }

                match wavelength {
                    w if w <= *min => points[0].clone(),
                    w if w >= *max => points.last().unwrap().clone(),
                    w => {
                        let normalized = (w - min) / (max - min);
                        let float_index = normalized * (points.len() as f32 - 1.0);
                        let min_float_index = float_index.trunc();

                        let min_index = min_float_index as usize;
                        let max_index = min_index + 1;

                        let min_value = points[min_index].clone();
                        let max_value = points[max_index].clone();

                        let mix = float_index - min_float_index;
                        min_value * (1.0 - mix) + max_value * mix
                    }
                }
            }
            Spectrum::Curve { points } => Interpolated { points }.get(wavelength),
        }
    }
}

pub struct Spectra {
    spectra: Vec<Spectrum<f32>>,
}

impl Spectra {
    pub fn new() -> Self {
        Spectra {
            spectra: Vec::new(),
        }
    }

    pub fn get(&self, id: SpectrumId) -> &Spectrum<f32> {
        self.spectra.get(id.0).expect("missing spectrum")
    }

    fn insert(&mut self, spectrum: Spectrum<f32>) -> SpectrumId {
        let id = self.spectra.len();
        self.spectra.push(spectrum);
        SpectrumId(id)
    }
}

pub struct SpectrumLoader {
    spectra: Spectra,
    table_map: HashMap<TableId, SpectrumId>,
}

impl SpectrumLoader {
    pub fn new() -> Self {
        SpectrumLoader {
            spectra: Spectra::new(),
            table_map: HashMap::new(),
        }
    }

    pub fn get(&self, id: TableId) -> Option<SpectrumId> {
        self.table_map.get(&id).cloned()
    }

    pub fn insert<'lua>(&mut self, id: TableId, spectrum: Spectrum<f32>) -> SpectrumId {
        match self.table_map.entry(id) {
            Entry::Occupied(entry) => *entry.get(),
            Entry::Vacant(entry) => {
                let id = self.spectra.insert(spectrum);
                entry.insert(id);
                id
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

impl<'a, 'lua> typed_nodes::FromLua<'lua, ParseContext<'a, 'lua>> for SpectrumId {
    fn from_lua(
        value: mlua::Value<'lua>,
        context: &mut ParseContext<'a, 'lua>,
    ) -> Result<Self, Box<dyn Error>> {
        typed_nodes::VisitTable::visit(value, context, |value, context| {
            let id = TableId::get_or_assign(&value, context)?;

            if let Some(spectrum) = context.get_spectrum_loader().get(id) {
                return Ok(spectrum);
            }

            let spectrum = if let Ok(name) = value.get::<_, String>("name") {
                match &*name {
                    "a" => light_source::A,
                    "d65" => light_source::D65,
                    _ => Err(format!("unknown builtin spectrum: {}", name))?,
                }
            } else {
                Spectrum::from_lua(mlua::Value::Table(value), context)?
            };

            Ok(context.get_spectrum_loader().insert(id, spectrum))
        })
    }
}
