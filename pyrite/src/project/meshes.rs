use std::{
    collections::{hash_map::Entry, HashMap},
    error::Error,
    path::{Path, PathBuf},
};

use obj::{Group, Obj, ObjData, Object};

pub struct Meshes {
    meshes: Vec<Obj>,
}

impl Meshes {
    fn new() -> Self {
        Meshes { meshes: Vec::new() }
    }

    fn insert(&mut self, mesh: Obj) -> MeshId {
        let id = MeshId(self.meshes.len());
        self.meshes.push(mesh);
        id
    }

    pub fn get(&self, id: MeshId) -> &Obj {
        self.meshes.get(id.0).expect("missing mesh")
    }
}

pub struct MeshLoader {
    meshes: Meshes,
    file_map: HashMap<PathBuf, MeshId>,
    project_dir: PathBuf,
}

impl MeshLoader {
    pub fn new(path: impl AsRef<Path>) -> Self {
        let project_dir = path.as_ref().into();

        MeshLoader {
            meshes: Meshes::new(),
            file_map: HashMap::new(),
            project_dir,
        }
    }

    pub fn load(&mut self, path: impl AsRef<Path>) -> Result<MeshId, Box<dyn Error>> {
        let path = self.project_dir.join(path).canonicalize()?;

        match self.file_map.entry(path) {
            Entry::Occupied(entry) => Ok(*entry.get()),
            Entry::Vacant(entry) => {
                let mesh = Obj::load(entry.key()).map_err(|error| {
                    format!("could not load {}: {}", entry.key().display(), error)
                })?;
                let mesh = remove_materials(mesh);
                let id = self.meshes.insert(mesh);
                entry.insert(id);
                Ok(id)
            }
        }
    }

    pub fn into_meshes(self) -> Meshes {
        self.meshes
    }
}

fn remove_materials(obj: Obj) -> Obj {
    let Obj {
        data:
            ObjData {
                position,
                texture,
                normal,
                objects,
                material_libs,
            },
        path,
    } = obj;

    Obj {
        data: ObjData {
            position,
            texture,
            normal,
            objects: objects
                .into_iter()
                .map(|object| {
                    let Object { name, groups } = object;
                    Object {
                        name,
                        groups: groups
                            .into_iter()
                            .map(|group| {
                                let Group {
                                    name, index, polys, ..
                                } = group;
                                Group {
                                    name,
                                    index,
                                    material: None,
                                    polys,
                                }
                            })
                            .collect(),
                    }
                })
                .collect(),
            material_libs,
        },
        path,
    }
}

#[derive(Debug, Copy, Clone, Ord, PartialOrd, Eq, PartialEq, Hash)]
#[repr(transparent)]
pub struct MeshId(usize);
