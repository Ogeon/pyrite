use std::path::Path;

use rand::Rng;

use config;
use config::entry::Entry;
use config::Prelude;

use cameras;
use lamp;
use materials;
use math;
use renderer;
use shapes;
use tracer;
use types3d;
use values;
use world;

macro_rules! try_parse {
    ($e:expr) => {
        match $e {
            Ok(v) => v,
            Err(e) => return ParseResult::InterpretError(e),
        }
    };

    ($e:expr, $under:expr) => {
        match $e {
            Ok(v) => v,
            Err(e) => return ParseResult::InterpretError(format!("{}: {}", $under, e)),
        }
    };
}

pub enum ParseResult<T> {
    Success(T),
    ParseError(config::Error),
    InterpretError(String),
}

pub fn from_file<P: AsRef<Path>, R: Rng + 'static>(path: P) -> ParseResult<Project<R>> {
    let mut context = Prelude::new();

    types3d::register_types(&mut context);
    world::register_types(&mut context);
    renderer::register_types(&mut context);
    cameras::register_types(&mut context);
    shapes::register_types::<R>(&mut context);
    lamp::register_types::<R>(&mut context);
    materials::register_types::<R>(&mut context);
    values::register_types(&mut context);
    math::register_types::<tracer::RenderContext>(&mut context);
    math::register_specific_types(&mut context);
    register_types(&mut context);

    let mut config = context.into_parser();
    if let Err(e) = config.parse_file(&path) {
        return ParseResult::ParseError(e);
    }
    let root = config.root().as_object().unwrap();

    println!("decoding image spec");
    let image_spec = match root.get("image") {
        Some(v) => try_parse!(decode_image_spec(v), "image"),
        None => return ParseResult::InterpretError("missing image specifications".into()),
    };

    println!("decoding renderer");
    let renderer = match root.get("renderer") {
        Some(v) => try_parse!(v.dynamic_decode(), "renderer"),
        None => return ParseResult::InterpretError("missing renderer specifications".into()),
    };

    println!("decoding camera");
    let camera = match root.get("camera") {
        Some(v) => try_parse!(v.dynamic_decode(), "camera"),
        None => return ParseResult::InterpretError("missing camera specifications".into()),
    };

    println!("decoding world");
    let world = match root.get("world") {
        Some(v) => try_parse!(
            world::decode_world(v, |source| {
                path.as_ref()
                    .parent()
                    .unwrap_or(path.as_ref())
                    .join(&source)
            }),
            "world"
        ),
        None => return ParseResult::InterpretError("missing world specifications".into()),
    };

    println!("the project has been decoded");

    ParseResult::Success(Project {
        image: image_spec,
        renderer: renderer,
        camera: camera,
        world: world,
    })
}

pub struct Project<R: Rng> {
    pub image: ImageSpec,
    pub renderer: renderer::Renderer,
    pub camera: cameras::Camera,
    pub world: world::World<R>,
}

pub struct ImageSpec {
    pub width: usize,
    pub height: usize,
    pub format: ImageFormat,
    pub rgb_curves: (Vec<(f64, f64)>, Vec<(f64, f64)>, Vec<(f64, f64)>),
}

fn decode_image_spec(entry: Entry) -> Result<ImageSpec, String> {
    let fields = try!(entry.as_object().ok_or("not an object".into()));

    let width = match fields.get("width") {
        Some(v) => try!(v.decode(), "width"),
        None => return Err("missing field 'width'".into()),
    };

    let height = match fields.get("height") {
        Some(v) => try!(v.decode(), "height"),
        None => return Err("missing field 'height'".into()),
    };

    let format = match fields.get("format") {
        Some(v) => try!(v.dynamic_decode(), "format"),
        None => return Err("missing field 'format'".into()),
    };

    let rgb_curves = match fields.get("rgb_curves") {
        Some(v) => try!(decode_rgb_curves(v), "rgb_curves"),
        None => return Err("missing field 'rgb_curves'".into()),
    };

    Ok(ImageSpec {
        width: width,
        height: height,
        format: format,
        rgb_curves: rgb_curves,
    })
}

fn decode_rgb_curves(
    entry: Entry,
) -> Result<(Vec<(f64, f64)>, Vec<(f64, f64)>, Vec<(f64, f64)>), String> {
    let fields = try!(entry.as_object().ok_or("not an object".into()));

    let red = match fields.get("red") {
        Some(v) => try!(v.decode(), "red"),
        None => return Err("missing field 'red'".into()),
    };

    let green = match fields.get("green") {
        Some(v) => try!(v.decode(), "green"),
        None => return Err("missing field 'green'".into()),
    };

    let blue = match fields.get("blue") {
        Some(v) => try!(v.decode(), "blue"),
        None => return Err("missing field 'blue'".into()),
    };

    Ok((red, green, blue))
}

fn register_types(context: &mut Prelude) {
    context
        .object("Image".into())
        .object("Png".into())
        .add_decoder(decode_png);
}

pub enum ImageFormat {
    Png,
}

fn decode_png(_: Entry) -> Result<ImageFormat, String> {
    Ok(ImageFormat::Png)
}
