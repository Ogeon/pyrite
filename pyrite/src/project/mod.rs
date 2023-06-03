use std::{collections::HashMap, error::Error, path::Path};

use mlua::Lua;

use cgmath::{Matrix4, SquareMatrix, Vector3};

use path_slash::PathBufExt;
use typed_nodes::Key;

use eval_context::{EvalContext, Evaluate};
use meshes::{MeshId, MeshLoader, Meshes};
use tables::Tables;
use textures::{TextureLoader, Textures};

use self::{
    parse_context::ParseContext,
    spectra::{Spectra, SpectrumLoader},
};

pub(crate) mod eval_context;
pub(crate) mod expressions;
pub(crate) mod materials;
pub(crate) mod meshes;
mod parse_context;
pub(crate) mod spectra;
mod tables;
pub(crate) mod textures;

pub(crate) fn load_project<'p, P: AsRef<Path>>(path: P) -> Result<ProjectData, Box<dyn Error>> {
    let project_dir = path
        .as_ref()
        .parent()
        .expect("could not get the project path parent directory");

    let lua = Lua::new();

    // Set up the preferred require path
    lua.load(&format!(
        r#"package.path = "{};" .. package.path"#,
        project_dir
            .join("?.lua")
            .to_slash()
            .expect("could not convert project path to UTF8")
    ))
    .set_name("<pyrite>")?
    .exec()?;

    // Register assign_id
    let tables = std::sync::Arc::new(Tables::new());
    let lua_tables = tables.clone();
    let assign_id =
        lua.create_function(move |_context, table: mlua::Table| lua_tables.assign_id(&table))?;
    lua.globals().set("assign_id", assign_id)?;

    // Load project building library
    lua.load(include_str!("lib.lua"))
        .set_name("<pyrite>/lib.lua")?
        .exec()?;

    // Run project file
    let project_file = std::fs::read_to_string(&path)?;
    let project = lua
        .load(&project_file)
        .set_name(
            path.as_ref()
                .file_name()
                .and_then(std::ffi::OsStr::to_str)
                .unwrap_or_else(|| "<project file>"),
        )?
        .eval()?;

    // Parse project config
    let mut nodes = Nodes::new();
    let mut meshes = MeshLoader::new(project_dir);
    let mut spectra = SpectrumLoader::new();
    let mut textures = TextureLoader::new(project_dir);
    let mut parse_context =
        ParseContext::new(&lua, &mut nodes, &mut textures, &mut meshes, &mut spectra);

    let project = typed_nodes::FromLua::from_lua(project, &mut parse_context)?;

    let meshes = meshes.into_meshes();
    let spectra = spectra.into_spectra();
    let textures = textures.into_textures();

    Ok(ProjectData {
        nodes,
        meshes,
        spectra,
        textures,
        project,
    })
}

pub(crate) struct ProjectData {
    pub nodes: Nodes,
    pub meshes: Meshes,
    pub spectra: Spectra,
    pub textures: Textures,
    pub project: Project,
}

#[derive(typed_nodes::FromLua)]
pub(crate) struct Project {
    pub(crate) image: Image,
    pub(crate) camera: Camera,
    pub(crate) renderer: Renderer,
    pub(crate) world: World,
}

#[derive(typed_nodes::FromLua)]
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub file: Option<String>,
    pub filter: Option<expressions::Expression>,
    pub white: Option<expressions::Expression>,
}

#[derive(typed_nodes::FromLua)]
pub enum Camera {
    Perspective {
        transform: Transform,

        fov: self::expressions::Expression,
        focus_distance: Option<self::expressions::Expression>,
        aperture: Option<self::expressions::Expression>,
    },
}

#[derive(typed_nodes::FromLua)]
pub enum Renderer {
    Simple {
        #[typed_nodes(flatten)]
        shared: RendererShared,
    },
    Bidirectional {
        #[typed_nodes(flatten)]
        shared: RendererShared,
        light_bounces: Option<u32>,
    },
    PhotonMapping {
        #[typed_nodes(flatten)]
        shared: RendererShared,
        radius: Option<f32>,
        photon_bounces: Option<u32>,
        photons: Option<usize>,
        photon_passes: Option<usize>,
    },
}

#[derive(typed_nodes::FromLua)]
pub struct RendererShared {
    pub threads: Option<usize>,
    pub bounces: Option<u32>,
    pub pixel_samples: u32,
    pub light_samples: Option<usize>,
    pub spectrum_samples: Option<u32>,
    pub spectrum_resolution: Option<usize>,
    pub tile_size: Option<usize>,
}

#[derive(typed_nodes::FromLua)]
pub(crate) struct World {
    pub(crate) sky: Option<self::expressions::Expression>,
    pub(crate) objects: Vec<WorldObject>,
}

#[derive(typed_nodes::FromLua)]
pub(crate) enum WorldObject {
    Sphere {
        position: self::expressions::Expression,
        radius: self::expressions::Expression,
        texture_scale: Option<self::expressions::Expression>,
        material: Material,
    },
    Plane {
        origin: self::expressions::Expression,
        normal: self::expressions::Expression,
        texture_scale: Option<self::expressions::Expression>,
        material: Material,
    },
    RayMarched {
        shape: Estimator,
        bounds: BoundingVolume,
        material: Material,
    },
    Mesh {
        file: MeshId,
        materials: HashMap<String, Material>,
        scale: Option<self::expressions::Expression>,
        transform: Option<Transform>,
    },
    DirectionalLight {
        direction: self::expressions::Expression,
        width: self::expressions::Expression,
        color: self::expressions::Expression,
    },
    PointLight {
        position: self::expressions::Expression,
        color: self::expressions::Expression,
    },
}

#[derive(typed_nodes::FromLua)]
pub enum BoundingVolume {
    Box {
        min: self::expressions::Expression,
        max: self::expressions::Expression,
    },
    Sphere {
        position: self::expressions::Expression,
        radius: self::expressions::Expression,
    },
}

#[derive(typed_nodes::FromLua)]
pub enum Estimator {
    Mandelbulb {
        iterations: self::expressions::Expression,
        threshold: self::expressions::Expression,
        power: self::expressions::Expression,
        constant: Option<self::expressions::Expression>,
    },
    QuaternionJulia {
        iterations: self::expressions::Expression,
        threshold: self::expressions::Expression,
        constant: self::expressions::Expression,
        slice_plane: self::expressions::Expression,
        variant: JuliaType,
    },
}

#[derive(typed_nodes::FromLua)]
pub struct JuliaType {
    pub name: String,
}

#[derive(typed_nodes::FromLua)]
pub(crate) struct Material {
    pub surface: Key<self::materials::SurfaceMaterial>,
    pub normal_map: Option<expressions::Expression>,
}

#[derive(typed_nodes::FromLua)]
pub enum Transform {
    LookAt {
        from: self::expressions::Expression,
        to: self::expressions::Expression,
        up: Option<self::expressions::Expression>,
    },
}

impl Evaluate<Matrix4<f32>> for Transform {
    fn evaluate<'a>(&self, context: EvalContext<'a>) -> Result<Matrix4<f32>, Box<dyn Error>> {
        Ok(match self {
            Transform::LookAt { from, to, up } => {
                let from = from.evaluate(context)?;
                let to = to.evaluate(context)?;
                let up: Option<_> = up.evaluate(context)?;
                let up = up.unwrap_or(Vector3::new(0.0, 1.0, 0.0));

                Matrix4::look_at(from, to, up)
                    .invert()
                    .ok_or("could not invert view matrix")?
            }
        })
    }
}

pub(crate) type Nodes = typed_nodes::Nodes<NodeId, typed_nodes::bounds::SendSyncBounds>;

#[derive(Hash, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct NodeId(typed_nodes::TableId);

impl From<typed_nodes::TableId> for NodeId {
    fn from(value: typed_nodes::TableId) -> Self {
        Self(value)
    }
}
