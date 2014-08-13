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
use math;
use values;

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
    let mut config = try_parse!(
        config::parse(config_src.as_slice().chars(), &mut |source| {
            let sub_path = Path::new(source.as_slice());

            let content = match File::open(&path.dir_path().join(&sub_path)).read_to_string() {
                Ok(s) => s,
                Err(e) => return Err(format!("error while reading {}: {}", path.display(), e))
            };
            
            let mut object_path = Vec::new();
            for comp in sub_path.with_extension("").str_components() {
                match comp {
                    Some(c) => object_path.push(String::from_str(c)),
                    None => return Ok((content, Vec::new()))
                }
            }

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
    pub rgb_curves: (Vec<(f64, f64)>, Vec<(f64, f64)>, Vec<(f64, f64)>)
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

            let rgb_curves = match fields.pop_equiv(&"rgb_curves") {
                Some(config::Structure(_, f)) => try!(decode_rgb_curves(f), "rgb_curves"),
                Some(_) => return Err(format!("expected a structure")),
                None => return Err(String::from_str("missing field 'rgb_curves'"))
            };

            Ok(ImageSpec {
                width: width,
                height: height,
                format: format,
                rgb_curves: rgb_curves
            })
        },
        config::Primitive(v) => Err(format!("unexpected {}", v)),
        config::List(_) => Err(format!("unexpected list"))
    }
}

fn decode_rgb_curves(fields: HashMap<String, config::ConfigItem>) -> Result<(Vec<(f64, f64)>, Vec<(f64, f64)>, Vec<(f64, f64)>), String> {
    let mut fields = fields;

    let red = match fields.pop_equiv(&"red") {
        Some(v) => try!(FromConfig::from_config(v), "red"),
        None => return Err(String::from_str("missing field 'red'"))
    };

    let green = match fields.pop_equiv(&"green") {
        Some(v) => try!(FromConfig::from_config(v), "green"),
        None => return Err(String::from_str("missing field 'green'"))
    };
        
    let blue = match fields.pop_equiv(&"blue") {
        Some(v) => try!(FromConfig::from_config(v), "blue"),
        None => return Err(String::from_str("missing field 'blue'"))
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
    Ok(Png)
}