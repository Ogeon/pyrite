use std::{
    collections::{hash_map::Entry, HashMap},
    error::Error,
};

use super::{
    expressions::Expression,
    parse_context::{Parse, ParseContext},
    tables::{TableExt, TableId},
};

pub(crate) struct Materials {
    materials: Vec<SurfaceMaterial>,
}

impl Materials {
    pub(crate) fn get(&self, id: MaterialId) -> &SurfaceMaterial {
        self.materials.get(id.0).expect("missing material")
    }
}

pub(crate) struct MaterialLoader<'lua> {
    materials: Vec<MaterialEntry<'lua>>,
    table_map: HashMap<TableId, MaterialId>,
    pending: Vec<MaterialId>,
}

impl<'lua> MaterialLoader<'lua> {
    pub fn new() -> Self {
        Self {
            materials: Vec::new(),
            table_map: HashMap::new(),
            pending: Vec::new(),
        }
    }

    pub fn insert(&mut self, table: rlua::Table<'lua>) -> Result<MaterialId, Box<dyn Error>> {
        let table_id = table.get_id()?;

        match self.table_map.entry(table_id) {
            Entry::Occupied(entry) => Ok(*entry.get()),
            Entry::Vacant(entry) => {
                let id = MaterialId(self.materials.len());
                self.materials.push(MaterialEntry::Pending(table));
                entry.insert(id);
                self.pending.push(id);
                Ok(id)
            }
        }
    }

    pub fn next_pending(&mut self) -> Option<(MaterialId, rlua::Table<'lua>)> {
        self.pending.pop().map(|id| {
            let table = self.materials[id.0].expect_pending();
            (id, table.clone())
        })
    }

    pub fn replace_pending(&mut self, id: MaterialId, material: SurfaceMaterial) {
        self.materials[id.0] = MaterialEntry::Parsed(material);
    }

    pub fn into_materials(self) -> Materials {
        Materials {
            materials: self
                .materials
                .into_iter()
                .map(MaterialEntry::into_parsed)
                .collect(),
        }
    }
}

enum MaterialEntry<'lua> {
    Parsed(SurfaceMaterial),
    Pending(rlua::Table<'lua>),
}

impl<'lua> MaterialEntry<'lua> {
    fn into_parsed(self) -> SurfaceMaterial {
        if let MaterialEntry::Parsed(material) = self {
            material
        } else {
            panic!("some materials were not parsed")
        }
    }

    fn expect_pending(&self) -> &rlua::Table<'lua> {
        if let MaterialEntry::Pending(table) = self {
            table
        } else {
            panic!("expected material to still be unparsed")
        }
    }
}

pub(crate) enum SurfaceMaterial {
    Emissive {
        color: Expression,
    },
    Diffuse {
        color: Expression,
    },
    Mirror {
        color: Expression,
    },
    Refractive {
        color: Expression,
        ior: Expression,
        dispersion: Option<Expression>,
        env_ior: Option<Expression>,
        env_dispersion: Option<Expression>,
    },
    Mix {
        lhs: MaterialId,
        rhs: MaterialId,
        amount: Expression,
    },
    Add {
        lhs: MaterialId,
        rhs: MaterialId,
    },
}
impl<'lua> Parse<'lua> for SurfaceMaterial {
    type Input = rlua::Table<'lua>;

    fn parse<'a>(mut context: ParseContext<'a, 'lua, Self::Input>) -> Result<Self, Box<dyn Error>> {
        let material_type = context.expect_field::<String>("type")?;

        match &*material_type {
            "emissive" => Ok(SurfaceMaterial::Emissive {
                color: context.parse_field("color")?,
            }),
            "diffuse" => Ok(SurfaceMaterial::Diffuse {
                color: context.parse_field("color")?,
            }),
            "mirror" => Ok(SurfaceMaterial::Mirror {
                color: context.parse_field("color")?,
            }),
            "refractive" => Ok(SurfaceMaterial::Refractive {
                color: context.parse_field("color")?,
                ior: context.parse_field("ior")?,
                env_ior: context.parse_field("env_ior")?,
                dispersion: context.parse_field("dispersion")?,
                env_dispersion: context.parse_field("env_dispersion")?,
            }),
            "mix" => Ok(SurfaceMaterial::Mix {
                amount: context.parse_field("amount")?,
                lhs: context.materials.insert(context.expect_field("lhs")?)?,
                rhs: context.materials.insert(context.expect_field("rhs")?)?,
            }),
            "binary" => {
                let operator: String = context.expect_field("operator")?;

                if operator != "add" {
                    return Err(format!("unexpected binary operator type: '{}'", operator).into());
                }

                Ok(SurfaceMaterial::Add {
                    lhs: context.materials.insert(context.expect_field("lhs")?)?,
                    rhs: context.materials.insert(context.expect_field("rhs")?)?,
                })
            }
            name => Err(format!("unexpected material type: '{}'", name).into()),
        }
    }
}

#[derive(Debug, Copy, Clone, Ord, PartialOrd, Eq, PartialEq, Hash)]
#[repr(transparent)]
pub(crate) struct MaterialId(usize);
