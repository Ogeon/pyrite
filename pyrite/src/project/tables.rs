use std::error::Error;

pub struct Tables {
    next_id: crossbeam::atomic::AtomicCell<u64>,
}

impl Tables {
    pub fn new() -> Self {
        Tables {
            next_id: crossbeam::atomic::AtomicCell::new(0),
        }
    }

    pub fn assign_id(&self, table: &rlua::Table) -> rlua::Result<()> {
        table.set("_id", self.next_id.fetch_add(1))?;

        Ok(())
    }
}

pub trait TableExt {
    fn get_id(&self) -> Result<TableId, Box<dyn Error>>;
    fn get_or_assign_id(&self, tables: &Tables) -> Result<TableId, Box<dyn Error>>;
}

impl<'lua> TableExt for rlua::Table<'lua> {
    fn get_id(&self) -> Result<TableId, Box<dyn Error>> {
        Ok(TableId(self.get("_id").map_err(|error| {
            format!("could not get table ID: {}", error)
        })?))
    }

    fn get_or_assign_id(&self, tables: &Tables) -> Result<TableId, Box<dyn Error>> {
        if let rlua::Value::Nil = self.get("_id")? {
            tables.assign_id(self)?;
        }

        self.get_id()
    }
}

#[derive(Debug, Copy, Clone, Ord, PartialOrd, Eq, PartialEq, Hash)]
#[repr(transparent)]
pub struct TableId(usize);
