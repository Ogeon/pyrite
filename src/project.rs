use std;
use std::io::Read;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::collections::HashMap;

use config;
use config::FromConfig;

use tracer;
use renderer;
use cameras;
use types3d;
use shapes;
use materials;
use math;
use values;

macro_rules! try_io {
    ($e:expr) => (
        match $e {
            Ok(v) => v,
            Err(e) => return ParseResult::IoError(e)
        }
    )
}

macro_rules! try_parse {
    ($e:expr) => (
        match $e {
            Ok(v) => v,
            Err(e) => return ParseResult::ParseError(e)
        }
    );

    ($e:expr, $under:expr) => (
        match $e {
            Ok(v) => v,
            Err(e) => return ParseResult::ParseError(format!("{}: {}", $under, e))
        }
    )
}

pub enum ParseResult<T> {
    Success(T),
    IoError(std::io::Error),
    ParseError(String)
}

pub fn from_file<P: AsRef<Path>>(path: P) -> ParseResult<Project> {
    let mut config_src = String::new();
    try_io!(File::open(&path).and_then(|mut f| f.read_to_string(&mut config_src)));
    let mut config = try_parse!(
        config::parse(config_src.chars(), &mut |source| {
            let sub_path = PathBuf::from(source);

            let mut content = String::new();
            let file_path = path.as_ref().parent().unwrap_or(path.as_ref()).join(&sub_path);
            if let Err(e) = File::open(&file_path).and_then(|mut f| f.read_to_string(&mut content)) {
                return Err(format!("error while reading {}: {}", file_path.display(), e));
            }
            
            let object_path = sub_path.with_extension("").components().map(|c| c.as_ref().to_string_lossy().into_owned()).collect();

            Ok((content, object_path))
        })
    );

    let mut context = config::ConfigContext::new();

    types3d::register_types(&mut context);
    tracer::register_types(&mut context);
    renderer::register_types(&mut context);
    cameras::register_types(&mut context);
    shapes::register_types(&mut context);
    materials::register_types(&mut context);
    values::register_types(&mut context);
    math::register_types::<tracer::RenderContext>(&mut context);
    math::register_specific_types(&mut context);
    register_types(&mut context);

    let image_spec = match config.remove("image") {
        Some(v) => try_parse!(decode_image_spec(&context, v), "image"),
        None => return ParseResult::ParseError("missing image specifications".into())
    };

    let renderer = match config.remove("renderer") {
        Some(v) => try_parse!(context.decode_structure_from_group("Renderer", v), "renderer"),
        None => return ParseResult::ParseError("missing renderer specifications".into())
    };

    let camera = match config.remove("camera") {
        Some(v) => try_parse!(context.decode_structure_from_group("Camera", v), "camera"),
        None => return ParseResult::ParseError("missing camera specifications".into())
    };

    let world = match config.remove("world") {
        Some(v) => try_parse!(tracer::decode_world(&context, v, |source| {
            path.as_ref().parent().unwrap_or(path.as_ref()).join(&source)
        }), "world"),
        None => return ParseResult::ParseError("missing world specifications".into())
    };

    ParseResult::Success(Project {
        image: image_spec,
        renderer: renderer,
        camera: camera,
        world: world
    })
}

pub struct Project {
    pub image: ImageSpec,
    pub renderer: renderer::Renderer,
    pub camera: cameras::Camera,
    pub world: tracer::World
}

pub struct ImageSpec {
    pub width: u32,
    pub height: u32,
    pub format: ImageFormat,
    pub rgb_curves: (Vec<(f64, f64)>, Vec<(f64, f64)>, Vec<(f64, f64)>)
}

fn decode_image_spec(context: &config::ConfigContext, item: config::ConfigItem) -> Result<ImageSpec, String> {
    match item {
        config::Structure(_, mut fields) => {
            let width = match fields.remove("width") {
                Some(v) => try!(FromConfig::from_config(v), "width"),
                None => return Err("missing field 'width'".into())
            };

            let height = match fields.remove("height") {
                Some(v) => try!(FromConfig::from_config(v), "height"),
                None => return Err("missing field 'height'".into())
            };

            let format = match fields.remove("format") {
                Some(v) => try!(context.decode_structure_from_group("Image", v), "format"),
                None => return Err("missing field 'format'".into())
            };

            let rgb_curves = match fields.remove("rgb_curves") {
                Some(config::Structure(_, f)) => try!(decode_rgb_curves(f), "rgb_curves"),
                Some(_) => return Err(format!("expected a structure")),
                None => return Err("missing field 'rgb_curves'".into())
            };

            Ok(ImageSpec {
                width: width,
                height: height,
                format: format,
                rgb_curves: rgb_curves
            })
        },
        config::Primitive(v) => Err(format!("unexpected {:?}", v)),
        config::List(_) => Err(format!("unexpected list"))
    }
}

fn decode_rgb_curves(fields: HashMap<String, config::ConfigItem>) -> Result<(Vec<(f64, f64)>, Vec<(f64, f64)>, Vec<(f64, f64)>), String> {
    let mut fields = fields;

    let red = match fields.remove("red") {
        Some(v) => try!(FromConfig::from_config(v), "red"),
        None => return Err("missing field 'red'".into())
    };

    let green = match fields.remove("green") {
        Some(v) => try!(FromConfig::from_config(v), "green"),
        None => return Err("missing field 'green'".into())
    };
        
    let blue = match fields.remove("blue") {
        Some(v) => try!(FromConfig::from_config(v), "blue"),
        None => return Err("missing field 'blue'".into())
    };

    Ok((red, green, blue))
}

fn register_types(context: &mut config::ConfigContext) {
    context.insert_grouped_type("Image", "Png", decode_png);
}

pub enum ImageFormat {
    Png
}

fn decode_png(_context: &config::ConfigContext, _items: HashMap<String, config::ConfigItem>) -> Result<ImageFormat, String> {
    Ok(ImageFormat::Png)
}