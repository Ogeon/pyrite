use std::io::{File, IoError};
use std::collections::HashMap;

use config;
use config::FromConfig;

use tracer;
use renderer;
use cameras;
use types3d;
use shapes;
use materials;

macro_rules! try_io(
    ($e:expr) => (
        match $e {
            Ok(v) => v,
            Err(e) => return IoError(e)
        }
    )
)

macro_rules! try_parse(
    ($e:expr) => (
        match $e {
            Ok(v) => v,
            Err(e) => return ParseError(e)
        }
    );

    ($e:expr, $under:expr) => (
        match $e {
            Ok(v) => v,
            Err(e) => return ParseError(format!("{}: {}", $under, e))
        }
    )
)

pub enum ParseResult<T> {
    Success(T),
    IoError(IoError),
    ParseError(String)
}

pub fn from_file(path: Path) -> ParseResult<Project> {
    let config_src = try_io!(File::open(&path).read_to_string());
    let mut config = try_parse!(config::parse(config_src.as_slice().chars()));

    let mut context = config::ConfigContext::new();

    types3d::register_types(&mut context);
    tracer::register_types(&mut context);
    renderer::register_types(&mut context);
    cameras::register_types(&mut context);
    shapes::register_types(&mut context);
    materials::register_types(&mut context);
    register_types(&mut context);

    let image_spec = match config.pop_equiv(&"image") {
        Some(v) => try_parse!(decode_image_spec(&context, v), "image"),
        None => return ParseError(String::from_str("missing image specifications"))
    };

    let renderer = match config.pop_equiv(&"renderer") {
        Some(v) => try_parse!(context.decode_structure_from_group("Renderer", v), "renderer"),
        None => return ParseError(String::from_str("missing renderer specifications"))
    };

    let camera = match config.pop_equiv(&"camera") {
        Some(v) => try_parse!(context.decode_structure_from_group("Camera", v), "camera"),
        None => return ParseError(String::from_str("missing camera specifications"))
    };

    let world = match config.pop_equiv(&"world") {
        Some(v) => try_parse!(tracer::decode_world(&context, v), "world"),
        None => return ParseError(String::from_str("missing world specifications"))
    };

    Success(Project {
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
    pub width: uint,
    pub height: uint,
    pub format: ImageFormat,
}

fn decode_image_spec(context: &config::ConfigContext, item: config::ConfigItem) -> Result<ImageSpec, String> {
    match item {
        config::Structure(_, mut fields) => {
            let width = match fields.pop_equiv(&"width") {
                Some(v) => try!(FromConfig::from_config(v), "width"),
                None => return Err(String::from_str("missing field 'width'"))
            };

            let height = match fields.pop_equiv(&"height") {
                Some(v) => try!(FromConfig::from_config(v), "height"),
                None => return Err(String::from_str("missing field 'height'"))
            };

            let format = match fields.pop_equiv(&"format") {
                Some(v) => try!(context.decode_structure_from_group("Image", v), "format"),
                None => return Err(String::from_str("missing field 'format'"))
            };

            Ok(ImageSpec {
                width: width,
                height: height,
                format: format
            })
        },
        config::Primitive(v) => Err(format!("unexpected {}", v)),
        config::List(_) => Err(format!("unexpected list"))
    }
}

fn register_types(context: &mut config::ConfigContext) {
    context.insert_grouped_type("Image", "Png", decode_png);
}

pub enum ImageFormat {
    Png
}

fn decode_png(_context: &config::ConfigContext, _items: HashMap<String, config::ConfigItem>) -> Result<ImageFormat, String> {
    Ok(Png)
}