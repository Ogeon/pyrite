use num_cpus;

use config::Prelude;
use config::entry::{Entry, Object};

use cameras;
use world;

use renderer::algorithm::Algorithm;

mod algorithm;
use film::Tile;

static DEFAULT_SPECTRUM_SAMPLES: u32 = 10;
static DEFAULT_SPECTRUM_BINS: usize = 64;
static DEFAULT_SPECTRUM_SPAN: (f64, f64) = (400.0, 700.0);

pub fn register_types(context: &mut Prelude) {
    let mut group = context.object("Renderer".into());
    group.object("Simple".into()).add_decoder(decode_simple);
    group.object("Bidirectional".into()).add_decoder(decode_bidirectional);
}

pub struct Renderer {
    pub threads: usize,
    bounces: u32,
    pixel_samples: u32,
    light_samples: usize,
    spectrum_samples: u32,
    pub spectrum_bins: usize,
    pub spectrum_span: (f64, f64),
    algorithm: Algorithm
}

impl Renderer {
    pub fn render_tile(&self, tile: &mut Tile, camera: &cameras::Camera, world: &world::World) {
        self.algorithm.render_tile(tile, camera, world, self)
    }

    pub fn tile_size(&self) -> usize {
        match self.algorithm {
            Algorithm::Simple { tile_size } => tile_size,
            Algorithm::Bidirectional { tile_size, .. } => tile_size,
        }
    }
}

fn decode_renderer(items: Object, algorithm: Algorithm) -> Result<Renderer, String> {
    let threads = match items.get("threads") {
        Some(v) => try!(v.decode(), "threads"),
        None => num_cpus::get()
    };

    let bounces = match items.get("bounces") {
        Some(v) => try!(v.decode(), "bounces"),
        None => 8
    };

    let pixel_samples = match items.get("pixel_samples") {
        Some(v) => try!(v.decode(), "pixel_samples"),
        None => 10
    };

    let light_samples = match items.get("light_samples") {
        Some(v) => try!(v.decode(), "light_samples"),
        None => 4
    };

    let (spectrum_samples, spectrum_bins, spectrum_span) = match items.get("spectrum").map(|e| e.as_object()) {
        Some(Some(v)) => try!(decode_spectrum(v), "spectrum"),
        Some(None) => return Err(format!("spectrum: expected a structure, but found something else")), //TODO: Print what we found
        None => (DEFAULT_SPECTRUM_SAMPLES, DEFAULT_SPECTRUM_BINS, DEFAULT_SPECTRUM_SPAN)
    };

    Ok(
        Renderer {
            threads: threads,
            bounces: bounces,
            pixel_samples: pixel_samples,
            light_samples: light_samples,
            spectrum_samples: spectrum_samples,
            spectrum_bins: spectrum_bins,
            spectrum_span: spectrum_span,
            algorithm: algorithm
        }
    )
}

fn decode_spectrum(items: Object) -> Result<(u32, usize, (f64, f64)), String> {
    let samples = match items.get("samples") {
        Some(v) => try!(v.decode(), "samples"),
        None => DEFAULT_SPECTRUM_SAMPLES
    };

    let bins = match items.get("bins") {
        Some(v) => try!(v.decode(), "bins"),
        None => DEFAULT_SPECTRUM_BINS
    };

    let span = match items.get("span") {
        Some(v) => try!(v.decode(), "span"),
        None => DEFAULT_SPECTRUM_SPAN
    };

    Ok((samples, bins, span))
}

fn decode_simple(entry: Entry) -> Result<Renderer, String> {
    let items = try!(entry.as_object().ok_or("not an object".into()));

    let tile_size = match items.get("tile_size") {
        Some(v) => try!(v.decode(), "tile_size"),
        None => 64
    };

    let algorithm = Algorithm::Simple {
        tile_size: tile_size,
    };

    decode_renderer(items, algorithm)
}

fn decode_bidirectional(entry: Entry) -> Result<Renderer, String> {
    let items = try!(entry.as_object().ok_or("not an object".into()));

    let tile_size = match items.get("tile_size") {
        Some(v) => try!(v.decode(), "tile_size"),
        None => 64
    };

    let light_bounces = match items.get("light_bounces") {
        Some(v) => try!(v.decode(), "light_bounces"),
        None => 8
    };

    let algorithm = Algorithm::Bidirectional {
        tile_size: tile_size,
        params: algorithm::BidirParams {
            bounces: light_bounces
        }
    };

    decode_renderer(items, algorithm)
}
