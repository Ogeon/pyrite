use std::{
    collections::{hash_map::Entry, HashMap},
    error::Error,
    path::{Path, PathBuf},
};

use palette::{LinLuma, LinSrgba};

use crate::texture::Texture;

pub struct Textures {
    color_textures: Vec<Texture<LinSrgba>>,
    mono_textures: Vec<Texture<LinLuma>>,
}

impl Textures {
    fn new() -> Self {
        Textures {
            color_textures: Vec::new(),
            mono_textures: Vec::new(),
        }
    }

    pub fn get_color(&self, id: ColorTextureId) -> &Texture<LinSrgba> {
        self.color_textures
            .get(id.0)
            .expect("missing color texture")
    }

    pub fn get_mono(&self, id: MonoTextureId) -> &Texture<LinLuma> {
        self.mono_textures.get(id.0).expect("missing mono texture")
    }

    fn insert_color(&mut self, texture: Texture<LinSrgba>) -> ColorTextureId {
        let id = self.color_textures.len();
        self.color_textures.push(texture);
        ColorTextureId(id)
    }

    fn insert_mono(&mut self, texture: Texture<LinLuma>) -> MonoTextureId {
        let id = self.mono_textures.len();
        self.mono_textures.push(texture);
        MonoTextureId(id)
    }
}

pub struct TextureLoader {
    textures: Textures,
    color_file_map: HashMap<PathBuf, ColorTextureId>,
    mono_file_map: HashMap<PathBuf, MonoTextureId>,
    project_dir: PathBuf,
}

impl TextureLoader {
    pub fn new(path: impl AsRef<Path>) -> Self {
        let project_dir = path.as_ref().into();

        TextureLoader {
            textures: Textures::new(),
            color_file_map: HashMap::new(),
            mono_file_map: HashMap::new(),
            project_dir,
        }
    }

    pub fn load_color(
        &mut self,
        path: impl AsRef<Path>,
        linear: bool,
    ) -> Result<ColorTextureId, Box<dyn Error>> {
        let path = self.project_dir.join(path).canonicalize()?;

        match self.color_file_map.entry(path) {
            Entry::Occupied(entry) => Ok(*entry.get()),
            Entry::Vacant(entry) => {
                let texture = Texture::from_path(entry.key(), linear).map_err(|error| {
                    format!(
                        "could not load {} as color texture: {}",
                        entry.key().display(),
                        error
                    )
                })?;
                let id = self.textures.insert_color(texture);
                entry.insert(id);
                Ok(id)
            }
        }
    }

    pub fn load_mono(
        &mut self,
        path: impl AsRef<Path>,
        linear: bool,
    ) -> Result<MonoTextureId, Box<dyn Error>> {
        let path = self.project_dir.join(path).canonicalize()?;

        match self.mono_file_map.entry(path) {
            Entry::Occupied(entry) => Ok(*entry.get()),
            Entry::Vacant(entry) => {
                let texture = Texture::from_path(entry.key(), linear).map_err(|error| {
                    format!(
                        "could not load {} as mono texture: {}",
                        entry.key().display(),
                        error
                    )
                })?;
                let id = self.textures.insert_mono(texture);
                entry.insert(id);
                Ok(id)
            }
        }
    }

    pub fn into_textures(self) -> Textures {
        self.textures
    }
}

#[derive(Debug, Copy, Clone, Ord, PartialOrd, Eq, PartialEq, Hash)]
#[repr(transparent)]
pub struct ColorTextureId(usize);

#[derive(Debug, Copy, Clone, Ord, PartialOrd, Eq, PartialEq, Hash)]
#[repr(transparent)]
pub struct MonoTextureId(usize);
