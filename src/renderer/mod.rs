use num_cpus;

use crossbeam::scope;
use simple_parallel::Pool;

use config::Prelude;
use config::entry::{Entry, Object};

use cameras;
use world;

use film::{Film, Tile};

mod algorithm;
mod photon_mapping;

static DEFAULT_SPECTRUM_SAMPLES: u32 = 10;
static DEFAULT_SPECTRUM_BINS: usize = 64;
static DEFAULT_SPECTRUM_SPAN: (f64, f64) = (400.0, 700.0);

pub fn register_types(context: &mut Prelude) {
    let mut group = context.object("Renderer".into());
    group.object("Simple".into()).add_decoder(decode_simple);
    group.object("Bidirectional".into()).add_decoder(decode_bidirectional);
    group.object("PhotonMapping".into()).add_decoder(decode_photon_mapping);
}

pub struct Renderer {
    pub threads: usize,
    bounces: u32,
    pixel_samples: u32,
    light_samples: usize,
    spectrum_samples: u32,
    pub spectrum_bins: usize,
    pub spectrum_span: (f64, f64),
    pub tile_size: usize,
    algorithm: Algorithm
}

impl Renderer {
    pub fn render<W: WorkPool, F: FnMut(Status)>(&self, film: &Film, workers: &mut W, on_status: F, camera: &cameras::Camera, world: &world::World) {
        match self.algorithm {
            Algorithm::Simple => {},
            Algorithm::Bidirectional(ref config) => {},
            Algorithm::PhotonMapping(ref config) => photon_mapping::render(film, workers, on_status, self, config, world, camera)
        }
    }
}

pub enum Algorithm {
    Simple,
    Bidirectional(algorithm::BidirParams),
    PhotonMapping(photon_mapping::Config),
}

pub trait WorkPool {
    fn do_work<I, T, U, W, R>(&mut self, work: I, worker: W, mut with_result: R) where
        T: Send,
        U: Send,
        I: Send,
        I: IntoIterator<Item = T>,
        W: Send + Sync,
        W: Fn(T) -> U,
        R: FnMut(usize, U);
}

impl WorkPool for Pool {
    fn do_work<I, T, U, W, R>(&mut self, work: I, worker: W, mut with_result: R) where
        T: Send,
        U: Send,
        I: Send,
        I: IntoIterator<Item = T>,
        W: Send + Sync,
        W: Fn(T) -> U,
        R: FnMut(usize, U)
    {
        scope(|scope| {
            for (index, result) in self.unordered_map(&scope, work, worker) {
                with_result(index, result);
            }
        });
    }
}

pub struct Status<'a> {
    pub progress: u8,
    pub message: &'a str
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

    let tile_size = match items.get("tile_size") {
        Some(v) => try!(v.decode(), "tile_size"),
        None => 64
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
            algorithm: algorithm,
            tile_size: tile_size,
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

    let algorithm = Algorithm::Simple;

    decode_renderer(items, algorithm)
}

fn decode_bidirectional(entry: Entry) -> Result<Renderer, String> {
    let items = try!(entry.as_object().ok_or("not an object".into()));

    let light_bounces = match items.get("light_bounces") {
        Some(v) => try!(v.decode(), "light_bounces"),
        None => 8
    };

    let algorithm = Algorithm::Bidirectional(
        algorithm::BidirParams {
            bounces: light_bounces
        }
    );

    decode_renderer(items, algorithm)
}

fn decode_photon_mapping(entry: Entry) -> Result<Renderer, String> {
    let items = try!(entry.as_object().ok_or("not an object".into()));

    let photons = match items.get("photons") {
        Some(v) => try!(v.decode(), "photons"),
        None => 10000
    };

    let photon_bounces = match items.get("photon_bounces") {
        Some(v) => try!(v.decode(), "photon_bounces"),
        None => 8
    };

    let photon_passes = match items.get("photon_passes") {
        Some(v) => try!(v.decode(), "photon_passes"),
        None => 1
    };

    let radius = match items.get("radius") {
        Some(v) => try!(v.decode(), "radius"),
        None => 0.1
    };

    let algorithm = Algorithm::PhotonMapping(
        photon_mapping::Config {
            photons: photons,
            photon_bounces: photon_bounces,
            photon_passes: photon_passes,
            radius: radius,
        }
    );

    decode_renderer(items, algorithm)
}
