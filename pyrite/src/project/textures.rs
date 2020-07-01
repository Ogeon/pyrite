use std::{
    collections::{hash_map::Entry, HashMap},
    error::Error,
    path::{Path, PathBuf},
};

use crate::texture::Texture;

pub struct Textures {
    textures: Vec<Texture>,
}

impl Textures {
    fn new() -> Self {
        Textures {
            textures: Vec::new(),
        }
    }

    pub fn get(&self, id: TextureId) -> &Texture {
        self.textures.get(id.0).expect("missing texture")
    }

    fn insert(&mut self, texture: Texture) -> TextureId {
        let id = self.textures.len();
        self.textures.push(texture);
        TextureId(id)
    }
}

pub struct TextureLoader {
    textures: Textures,
    file_map: HashMap<PathBuf, TextureId>,
    project_dir: PathBuf,
}

impl TextureLoader {
    pub fn new(path: impl AsRef<Path>) -> Result<Self, Box<dyn Error>> {
        let project_dir = path.as_ref().canonicalize()?;

        Ok(TextureLoader {
            textures: Textures::new(),
            file_map: HashMap::new(),
            project_dir,
        })
    }

    pub fn load(&mut self, path: impl AsRef<Path>) -> Result<TextureId, Box<dyn Error>> {
        let path = self.project_dir.join(path);

        match self.file_map.entry(path) {
            Entry::Occupied(entry) => Ok(*entry.get()),
            Entry::Vacant(entry) => {
                let texture = Texture::from_path(entry.key()).map_err(|error| {
                    format!("could not load {}: {}", entry.key().display(), error)
                })?;
                let id = self.textures.insert(texture);
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
pub struct TextureId(usize);
