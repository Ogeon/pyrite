use std::{collections::HashMap, error::Error, path::Path};

use rlua::{FromLua, Lua};

use cgmath::{Matrix4, SquareMatrix, Vector3};

use path_slash::PathBufExt;

use crate::parse_enum;

use eval_context::{EvalContext, Evaluate};
use expressions::{ExpressionLoader, Expressions};
use meshes::{MeshId, MeshLoader, Meshes};
use parse_context::{Parse, ParseContext};
use spectra::{Spectra, SpectrumLoader};
use tables::Tables;
use textures::{TextureLoader, Textures};

pub mod eval_context;
pub mod expressions;
pub mod meshes;
mod parse_context;
pub mod spectra;
mod tables;
pub(crate) mod textures;

pub fn load_project<'p, P: AsRef<Path>>(path: P) -> Result<ProjectData, Box<dyn Error>> {
    let project_dir = path
        .as_ref()
        .parent()
        .expect("could not get the project path parent directory");

    let lua = Lua::new();

    lua.context(|context| {
        // Set up the preferred require path
        context
            .load(&format!(
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
        let assign_id = context
            .create_function(move |_context, table: rlua::Table| lua_tables.assign_id(&table))?;
        context.globals().set("assign_id", assign_id)?;

        // Load project building library
        context
            .load(include_str!("lib.lua"))
            .set_name("<pyrite>/lib.lua")?
            .exec()?;

        // Run project file
        let project_file = std::fs::read_to_string(&path)?;
        let project = context
            .load(&project_file)
            .set_name(
                path.as_ref()
                    .file_name()
                    .and_then(std::ffi::OsStr::to_str)
                    .unwrap_or_else(|| "<project file>"),
            )?
            .eval()?;

        // Parse project config
        let mut expressions = ExpressionLoader::new();
        let mut meshes = MeshLoader::new(project_dir);
        let mut spectra = SpectrumLoader::new();
        let mut textures = TextureLoader::new(project_dir);
        let parse_context = ParseContext::new(
            &mut expressions,
            &mut meshes,
            &mut spectra,
            &mut textures,
            &tables,
            rlua::Table::from_lua(project, context.clone())?,
            &context,
        );
        let project = parse_context.parse()?;
        while let Some((id, table)) = expressions.next_pending() {
            let expression = ParseContext::new(
                &mut expressions,
                &mut meshes,
                &mut spectra,
                &mut textures,
                &tables,
                table,
                &context,
            )
            .parse()?;
            expressions.replace_pending(id, expression);
        }

        let expressions = expressions.into_expressions();
        let meshes = meshes.into_meshes();
        let spectra = spectra.into_spectra();
        let textures = textures.into_textures();

        Ok(ProjectData {
            expressions,
            meshes,
            spectra,
            textures,
            project,
        })
    })
}

pub struct ProjectData {
    pub expressions: Expressions,
    pub meshes: Meshes,
    pub spectra: Spectra,
    pub textures: Textures,
    pub project: Project,
}

pub struct Project {
    pub image: Image,
    pub camera: Camera,
    pub renderer: Renderer,
    pub world: World,
}

impl<'lua> Parse<'lua> for Project {
    type Input = rlua::Table<'lua>;

    fn parse<'a>(mut context: ParseContext<'a, 'lua, Self::Input>) -> Result<Self, Box<dyn Error>> {
        Ok(Project {
            image: context.parse_field("image")?,
            camera: context.parse_field("camera")?,
            renderer: context.parse_field("renderer")?,
            world: context.parse_field("world")?,
        })
    }
}

pub struct Image {
    pub width: u32,
    pub height: u32,
    pub file: Option<String>,
    pub filter: Option<expressions::Expression>,
    pub white: Option<expressions::Expression>,
}

impl<'lua> Parse<'lua> for Image {
    type Input = rlua::Table<'lua>;

    fn parse<'a>(mut context: ParseContext<'a, 'lua, Self::Input>) -> Result<Self, Box<dyn Error>> {
        Ok(Image {
            width: context.expect_field("width")?,
            height: context.expect_field("height")?,
            file: context.expect_field("file")?,
            filter: context.parse_field("filter")?,
            white: context.parse_field("white")?,
        })
    }
}

pub enum Camera {
    Perspective {
        transform: Transform,
        fov: self::expressions::Expression,
        focus_distance: Option<self::expressions::Expression>,
        aperture: Option<self::expressions::Expression>,
    },
}

impl<'lua> Parse<'lua> for Camera {
    type Input = rlua::Table<'lua>;

    fn parse<'a>(mut context: ParseContext<'a, 'lua, Self::Input>) -> Result<Self, Box<dyn Error>> {
        parse_enum!(context {
            "perspective" => Ok(Camera::Perspective {
                transform: context.parse_field("transform")?,
                fov: context.parse_field("fov")?,
                focus_distance: context.parse_field("focus_distance")?,
                aperture: context.parse_field("aperture")?,
            }),
        })
    }
}

pub enum Renderer {
    Simple {
        shared: RendererShared,
    },
    Bidirectional {
        shared: RendererShared,
        light_bounces: Option<u32>,
    },
    PhotonMapping {
        shared: RendererShared,
        radius: Option<f32>,
        photon_bounces: Option<u32>,
        photons: Option<usize>,
        photon_passes: Option<usize>,
    },
}

impl<'lua> Parse<'lua> for Renderer {
    type Input = rlua::Table<'lua>;

    fn parse<'a>(mut context: ParseContext<'a, 'lua, Self::Input>) -> Result<Self, Box<dyn Error>> {
        let shared = RendererShared::parse(&mut context)?;

        parse_enum!(context {
            "simple" => Ok(Renderer::Simple {
                shared,
            }),
            "bidirectional" => Ok(Renderer::Bidirectional {
                shared,
                light_bounces: context.expect_field("light_bounces")?,
            }),
            "photon_mapping" => Ok(Renderer::PhotonMapping {
                shared,
                radius: context.expect_field("radius")?,
                photon_bounces: context.expect_field("photon_bounces")?,
                photons: context.expect_field("photons")?,
                photon_passes: context.expect_field("photon_passes")?,
            })
        })
    }
}

pub struct RendererShared {
    pub threads: Option<usize>,
    pub bounces: Option<u32>,
    pub pixel_samples: u32,
    pub light_samples: Option<usize>,
    pub spectrum_samples: Option<u32>,
    pub spectrum_resolution: Option<usize>,
    pub tile_size: Option<usize>,
}

impl RendererShared {
    fn parse<'a, 'lua>(
        context: &mut ParseContext<'a, 'lua, rlua::Table<'lua>>,
    ) -> Result<Self, Box<dyn Error>> {
        Ok(RendererShared {
            threads: context.expect_field("threads")?,
            bounces: context.expect_field("bounces")?,
            pixel_samples: context.expect_field("pixel_samples")?,
            light_samples: context.expect_field("light_samples")?,
            spectrum_samples: context.expect_field("spectrum_samples")?,
            spectrum_resolution: context.expect_field("spectrum_resolution")?,
            tile_size: context.expect_field("tile_size")?,
        })
    }
}

pub struct World {
    pub sky: Option<self::expressions::Expression>,
    pub objects: Vec<WorldObject>,
}

impl<'lua> Parse<'lua> for World {
    type Input = rlua::Table<'lua>;

    fn parse<'a>(mut context: ParseContext<'a, 'lua, Self::Input>) -> Result<Self, Box<dyn Error>> {
        Ok(World {
            sky: context.parse_field("sky")?,
            objects: context.parse_array_field("objects")?,
        })
    }
}

pub enum WorldObject {
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

impl<'lua> Parse<'lua> for WorldObject {
    type Input = rlua::Table<'lua>;

    fn parse<'a>(mut context: ParseContext<'a, 'lua, Self::Input>) -> Result<Self, Box<dyn Error>> {
        parse_enum!(context {
            "sphere" => Ok(WorldObject::Sphere {
                position: context.parse_field("position")?,
                radius: context.parse_field("radius")?,
                texture_scale: context.parse_field("texture_scale")?,
                material: context.parse_field("material")?,
            }),
            "plane" => Ok(WorldObject::Plane {
                origin: context.parse_field("origin")?,
                normal: context.parse_field("normal")?,
                texture_scale: context.parse_field("texture_scale")?,
                material: context.parse_field("material")?,
            }),
            "ray_marched" => Ok(WorldObject::RayMarched {
                shape: context.parse_field("shape")?,
                bounds: context.parse_field("bounds")?,
                material: context.parse_field("material")?,
            }),
            "mesh" => Ok(WorldObject::Mesh {
                file: context.meshes.load(context.expect_field::<String>("file")?)?,
                materials: context.parse_map_field("materials")?,
                scale: context.parse_field("scale")?,
                transform: context.parse_field("transform")?,
            }),
            "directional_light" => Ok(WorldObject::DirectionalLight {
                direction: context.parse_field("direction")?,
                width: context.parse_field("width")?,
                color: context.parse_field("color")?,
            }),
            "point_light" => Ok(WorldObject::PointLight {
                position: context.parse_field("position")?,
                color: context.parse_field("color")?,
            }),
        })
    }
}

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

impl<'lua> Parse<'lua> for BoundingVolume {
    type Input = rlua::Table<'lua>;

    fn parse<'a>(mut context: ParseContext<'a, 'lua, Self::Input>) -> Result<Self, Box<dyn Error>> {
        parse_enum!(context {
            "box" => Ok(BoundingVolume::Box {
                min: context.parse_field("min")?,
                max: context.parse_field("max")?,
            }),
            "sphere" => Ok(BoundingVolume::Sphere {
                position: context.parse_field("position")?,
                radius: context.parse_field("radius")?,
            }),
        })
    }
}

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

impl<'lua> Parse<'lua> for Estimator {
    type Input = rlua::Table<'lua>;

    fn parse<'a>(mut context: ParseContext<'a, 'lua, Self::Input>) -> Result<Self, Box<dyn Error>> {
        parse_enum!(context {
            "mandelbulb" => Ok(Estimator::Mandelbulb {
                iterations: context.parse_field("iterations")?,
                threshold: context.parse_field("threshold")?,
                power: context.parse_field("power")?,
                constant: context.parse_field("constant")?,
            }),
            "quaternion_julia" => Ok(Estimator::QuaternionJulia {
                iterations: context.parse_field("iterations")?,
                threshold: context.parse_field("threshold")?,
                constant: context.parse_field("constant")?,
                slice_plane: context.parse_field("slice_plane")?,
                variant: context.parse_field("variant")?,
            }),
        })
    }
}

pub struct JuliaType {
    pub name: String,
}

impl<'lua> Parse<'lua> for JuliaType {
    type Input = rlua::Table<'lua>;

    fn parse<'a>(context: ParseContext<'a, 'lua, Self::Input>) -> Result<Self, Box<dyn Error>> {
        Ok(JuliaType {
            name: context.expect_field("name")?,
        })
    }
}

pub struct Material {
    pub surface: SurfaceMaterial,
    pub normal_map: Option<expressions::Expression>,
}

impl<'lua> Parse<'lua> for Material {
    type Input = rlua::Table<'lua>;

    fn parse<'a>(mut context: ParseContext<'a, 'lua, Self::Input>) -> Result<Self, Box<dyn Error>> {
        Ok(Material {
            surface: context.parse_field("surface")?,
            normal_map: context.parse_field("normal_map")?,
        })
    }
}

pub enum SurfaceMaterial {
    Diffuse {
        color: self::expressions::Expression,
    },
    Emission {
        color: self::expressions::Expression,
    },
    Mirror {
        color: self::expressions::Expression,
    },
    Refractive {
        color: self::expressions::Expression,
        ior: self::expressions::Expression,
        dispersion: Option<self::expressions::Expression>,
        env_ior: Option<self::expressions::Expression>,
        env_dispersion: Option<self::expressions::Expression>,
    },
    Mix {
        amount: self::expressions::Expression,
        lhs: Box<SurfaceMaterial>,
        rhs: Box<SurfaceMaterial>,
    },
    FresnelMix {
        ior: self::expressions::Expression,
        dispersion: Option<self::expressions::Expression>,
        env_ior: Option<self::expressions::Expression>,
        env_dispersion: Option<self::expressions::Expression>,
        reflect: Box<SurfaceMaterial>,
        refract: Box<SurfaceMaterial>,
    },
}

impl<'lua> Parse<'lua> for SurfaceMaterial {
    type Input = rlua::Table<'lua>;

    fn parse<'a>(mut context: ParseContext<'a, 'lua, Self::Input>) -> Result<Self, Box<dyn Error>> {
        parse_enum!(context {
            "diffuse" => Ok(SurfaceMaterial::Diffuse {
                color: context.parse_field("color")?,
            }),
            "emission" => Ok(SurfaceMaterial::Emission {
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
                lhs: Box::new(context.parse_field("lhs")?),
                rhs: Box::new(context.parse_field("rhs")?),
            }),
            "fresnel_mix" => Ok(SurfaceMaterial::FresnelMix {
                ior: context.parse_field("ior")?,
                env_ior: context.parse_field("env_ior")?,
                dispersion: context.parse_field("dispersion")?,
                env_dispersion: context.parse_field("env_dispersion")?,
                reflect: Box::new(context.parse_field("reflect")?),
                refract: Box::new(context.parse_field("refract")?),
            }),
        })
    }
}

pub enum Transform {
    LookAt {
        from: self::expressions::Expression,
        to: self::expressions::Expression,
        up: Option<self::expressions::Expression>,
    },
}

impl<'lua> Parse<'lua> for Transform {
    type Input = rlua::Table<'lua>;

    fn parse<'a>(mut context: ParseContext<'a, 'lua, Self::Input>) -> Result<Self, Box<dyn Error>> {
        parse_enum!(context {
            "look_at" => Ok(Transform::LookAt {
                from: context.parse_field("from")?,
                to: context.parse_field("to")?,
                up: context.parse_field("up")?,
            })
        })
    }
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
