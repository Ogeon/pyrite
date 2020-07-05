use std::{collections::HashMap, error::Error};

use rlua::FromLua;

use super::{
    expressions::ExpressionLoader, meshes::MeshLoader, spectra::SpectrumLoader, tables::Tables,
    textures::TextureLoader,
};

pub struct ParseContext<'a, 'lua: 'a, T> {
    pub expressions: &'a mut ExpressionLoader<'lua>,
    pub meshes: &'a mut MeshLoader,
    pub spectra: &'a mut SpectrumLoader,
    pub textures: &'a mut TextureLoader,
    pub tables: &'a Tables,

    current_value: T,
    context: &'a rlua::Context<'lua>,
}

impl<'a, 'lua, T: FromLua<'lua>> ParseContext<'a, 'lua, T> {
    pub fn new(
        expressions: &'a mut ExpressionLoader<'lua>,
        meshes: &'a mut MeshLoader,
        spectra: &'a mut SpectrumLoader,
        textures: &'a mut TextureLoader,
        tables: &'a Tables,
        value: T,
        context: &'a rlua::Context<'lua>,
    ) -> Self {
        ParseContext {
            expressions,
            meshes,
            spectra,
            textures,
            tables,
            current_value: value,
            context,
        }
    }
    pub fn value(&self) -> &T {
        &self.current_value
    }

    pub fn parse<U: Parse<'lua, Input = T>>(self) -> Result<U, Box<dyn Error>> {
        U::parse(self)
    }

    pub fn clone(&mut self) -> ParseContext<'_, 'lua, T>
    where
        T: Clone,
    {
        ParseContext {
            expressions: self.expressions,
            meshes: self.meshes,
            spectra: self.spectra,
            textures: self.textures,
            tables: self.tables,

            current_value: self.current_value.clone(),
            context: self.context,
        }
    }
}

impl<'a, 'lua> ParseContext<'a, 'lua, rlua::Value<'lua>> {
    pub fn expect_number(&self) -> Result<f64, Box<dyn Error>> {
        Ok(f64::from_lua(
            self.current_value.clone(),
            self.context.clone(),
        )?)
    }

    pub fn expect_table(&self) -> Result<rlua::Table<'lua>, Box<dyn Error>> {
        if let rlua::Value::Table(table) = &self.current_value {
            Ok(table.clone())
        } else {
            Err("expected a table".into())
        }
    }

    pub fn narrow<U: FromLua<'lua>>(self) -> Result<ParseContext<'a, 'lua, U>, Box<dyn Error>> {
        Ok(ParseContext {
            expressions: self.expressions,
            meshes: self.meshes,
            spectra: self.spectra,
            textures: self.textures,
            tables: self.tables,

            current_value: U::from_lua(self.current_value, self.context.clone())?,
            context: self.context,
        })
    }
}

impl<'a, 'lua> ParseContext<'a, 'lua, rlua::Table<'lua>> {
    pub fn expect_field<T: FromLua<'lua>>(&self, name: &str) -> Result<T, Box<dyn Error>> {
        Ok(self
            .current_value
            .get(name)
            .map_err(|error| format!("{}: {}", name, error))?)
    }

    pub fn with_field<T: FromLua<'lua>, U>(
        &mut self,
        name: &str,
        parse: impl FnOnce(ParseContext<'_, 'lua, T>) -> Result<U, Box<dyn Error>>,
    ) -> Result<U, Box<dyn Error>> {
        let input = self.expect_field(name)?;

        let new_context = ParseContext {
            expressions: self.expressions,
            meshes: self.meshes,
            spectra: self.spectra,
            textures: self.textures,
            tables: self.tables,

            current_value: input,
            context: self.context,
        };

        parse(new_context).map_err(|error| format!("{}: {}", name, error).into())
    }

    pub fn parse_field<T: Parse<'lua>>(&mut self, name: &str) -> Result<T, Box<dyn Error>> {
        self.with_field(name, T::parse)
    }

    pub fn parse_array_field<T: Parse<'lua>>(
        &mut self,
        name: &str,
    ) -> Result<Vec<T>, Box<dyn Error>> {
        self.with_field(
            name,
            |context: ParseContext<'_, 'lua, rlua::Table<'lua>>| {
                let ParseContext {
                    expressions,
                    meshes,
                    spectra,
                    textures,
                    tables,
                    current_value,
                    context,
                } = context;

                current_value
                    .sequence_values()
                    .enumerate()
                    .map(|(index, value)| {
                        let value = value?;

                        ParseContext {
                            expressions,
                            meshes,
                            spectra,
                            textures,
                            tables,

                            current_value: value,
                            context,
                        }
                        .narrow()
                        .map_err(|error| format!("[{}]: {}", index + 1, error))?
                        .parse()
                        .map_err(|error| format!("[{}]: {}", index + 1, error).into())
                    })
                    .collect()
            },
        )
    }

    pub fn parse_map_field<T: Parse<'lua>>(
        &mut self,
        name: &str,
    ) -> Result<HashMap<String, T>, Box<dyn Error>> {
        self.with_field(
            name,
            |context: ParseContext<'_, 'lua, rlua::Table<'lua>>| {
                let ParseContext {
                    expressions,
                    meshes,
                    spectra,
                    textures,
                    tables,
                    current_value,
                    context,
                } = context;

                current_value
                    .pairs()
                    .map(|pair| {
                        let (key, value) = pair?;

                        let context = ParseContext {
                            expressions,
                            meshes,
                            spectra,
                            textures,
                            tables,

                            current_value: value,
                            context,
                        };

                        let value = context
                            .narrow()
                            .map_err(|error| format!("{}: {}", key, error))?
                            .parse()
                            .map_err(|error| format!("{}: {}", key, error))?;

                        Ok((key, value))
                    })
                    .collect()
            },
        )
    }
}

pub trait Parse<'lua>: Sized {
    type Input: FromLua<'lua>;

    fn parse<'a>(context: ParseContext<'a, 'lua, Self::Input>) -> Result<Self, Box<dyn Error>>;
}

impl<'lua, T: Parse<'lua>> Parse<'lua> for Option<T> {
    type Input = rlua::Value<'lua>;

    fn parse<'a>(context: ParseContext<'a, 'lua, Self::Input>) -> Result<Self, Box<dyn Error>> {
        if let &rlua::Value::Nil = context.value() {
            Ok(None)
        } else {
            context.narrow()?.parse().map(Some)
        }
    }
}

#[macro_export]
macro_rules! parse_enum {
    ($context:ident {$($variant:literal => $result:expr),*$(,)?}) => {
        {
            let variant = $context.expect_field::<String>("type")?;

            match &*variant {
                $($variant => $result,)*
                other => return Err(format!("unexpected variant '{}'", other).into()),
            }
        }
    };
    ($context:ident[$key:literal] {$($variant:literal => $result:expr),*$(,)?}) => {
        {
            let variant = $context.expect_field::<String>($key)?;

            match &*variant {
                $($variant => $result,)*
                other => return Err(format!("unexpected value for {}: '{}'", $key, other).into()),
            }
        }
    };
}
