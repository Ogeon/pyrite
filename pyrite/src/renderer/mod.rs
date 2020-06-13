use num_cpus;

use crate::config::entry::{Entry, Object};
use crate::config::Prelude;

use crate::cameras;
use crate::world;

use crate::film::Film;
use std::path::Path;

mod algorithm;
mod bidirectional;
mod photon_mapping;
mod simple;

static DEFAULT_SPECTRUM_SAMPLES: u32 = 10;
static DEFAULT_SPECTRUM_BINS: usize = 64;
static DEFAULT_SPECTRUM_SPAN: (f32, f32) = (380.0, 780.0);

pub fn register_types(context: &mut Prelude) {
    let mut group = context.object("Renderer".into());
    group.object("Simple".into()).add_decoder(decode_simple);
    group
        .object("Bidirectional".into())
        .add_decoder(decode_bidirectional);
    group
        .object("PhotonMapping".into())
        .add_decoder(decode_photon_mapping);
}

pub struct Renderer {
    pub threads: usize,
    bounces: u32,
    pixel_samples: u32,
    light_samples: usize,
    spectrum_samples: u32,
    pub spectrum_bins: usize,
    pub spectrum_span: (f32, f32),
    pub tile_size: usize,
    algorithm: Algorithm,
}

impl Renderer {
    pub fn render<W: WorkPool, F: FnMut(Status<'_>)>(
        &self,
        film: &Film,
        workers: &mut W,
        on_status: F,
        camera: &cameras::Camera,
        world: &world::World,
    ) {
        match self.algorithm {
            Algorithm::Simple => simple::render(film, workers, on_status, self, world, camera),
            Algorithm::Bidirectional(ref config) => {
                bidirectional::render(film, workers, on_status, self, config, world, camera)
            }
            Algorithm::PhotonMapping(ref config) => {
                photon_mapping::render(film, workers, on_status, self, config, world, camera)
            }
        }
    }
}

pub enum Algorithm {
    Simple,
    Bidirectional(bidirectional::BidirParams),
    PhotonMapping(photon_mapping::Config),
}

pub trait WorkPool {
    fn do_work<I, T, U, W, R>(&mut self, work: I, worker: W, with_result: R)
    where
        T: Send,
        U: Send,
        I: Send,
        I: IntoIterator<Item = T>,
        I::IntoIter: Iterator + Send,
        W: Send + Sync,
        W: Fn(T) -> U,
        R: FnMut(usize, U);
}

pub struct RayonPool;
impl WorkPool for RayonPool {
    fn do_work<I, T, U, W, R>(&mut self, work: I, worker: W, mut with_result: R)
    where
        T: Send,
        U: Send,
        I: Send,
        I: IntoIterator<Item = T>,
        I::IntoIter: Iterator + Send,
        W: Send + Sync,
        W: Fn(T) -> U,
        R: FnMut(usize, U),
    {
        use rayon::prelude::*;

        let (sender, receiver) = crossbeam::channel::unbounded();

        crossbeam::scope(|scope| {
            scope.spawn(move |_| {
                work.into_iter()
                    .enumerate()
                    .par_bridge()
                    .for_each(|(index, input)| sender.send((index, worker(input))).unwrap());
            });

            for (index, result) in receiver {
                with_result(index, result);
            }
        })
        .unwrap();

        /*scope(|scope| {
            for (index, result) in self.unordered_map(&scope, work, worker) {
                with_result(index, result);
            }
        });*/
    }
}

pub struct Status<'a> {
    pub progress: u8,
    pub message: &'a str,
}

fn decode_renderer(items: Object<'_>, algorithm: Algorithm) -> Result<Renderer, String> {
    let threads = match items.get("threads") {
        Some(v) => try_for!(v.decode(), "threads"),
        None => num_cpus::get(),
    };

    let bounces = match items.get("bounces") {
        Some(v) => try_for!(v.decode(), "bounces"),
        None => 8,
    };

    let pixel_samples = match items.get("pixel_samples") {
        Some(v) => try_for!(v.decode(), "pixel_samples"),
        None => 10,
    };

    let light_samples = match items.get("light_samples") {
        Some(v) => try_for!(v.decode(), "light_samples"),
        None => 4,
    };

    let tile_size = match items.get("tile_size") {
        Some(v) => try_for!(v.decode(), "tile_size"),
        None => 64,
    };

    let (spectrum_samples, spectrum_bins, spectrum_span) =
        match items.get("spectrum").map(|e| e.as_object()) {
            Some(Some(v)) => try_for!(decode_spectrum(v), "spectrum"),
            Some(None) => {
                return Err(format!(
                    "spectrum: expected a structure, but found something else"
                ))
            } //TODO: Print what we found
            None => (
                DEFAULT_SPECTRUM_SAMPLES,
                DEFAULT_SPECTRUM_BINS,
                DEFAULT_SPECTRUM_SPAN,
            ),
        };

    Ok(Renderer {
        threads: threads,
        bounces: bounces,
        pixel_samples: pixel_samples,
        light_samples: light_samples,
        spectrum_samples: spectrum_samples,
        spectrum_bins: spectrum_bins,
        spectrum_span: spectrum_span,
        algorithm: algorithm,
        tile_size: tile_size,
    })
}

fn decode_spectrum(items: Object<'_>) -> Result<(u32, usize, (f32, f32)), String> {
    let samples = match items.get("samples") {
        Some(v) => try_for!(v.decode(), "samples"),
        None => DEFAULT_SPECTRUM_SAMPLES,
    };

    let bins = match items.get("bins") {
        Some(v) => try_for!(v.decode(), "bins"),
        None => DEFAULT_SPECTRUM_BINS,
    };

    let span = match items.get("span") {
        Some(v) => try_for!(v.decode(), "span"),
        None => DEFAULT_SPECTRUM_SPAN,
    };

    Ok((samples, bins, span))
}

fn decode_simple(_path: &'_ Path, entry: Entry<'_>) -> Result<Renderer, String> {
    let items = entry.as_object().ok_or("not an object")?;

    let algorithm = Algorithm::Simple;

    decode_renderer(items, algorithm)
}

fn decode_bidirectional(_path: &'_ Path, entry: Entry<'_>) -> Result<Renderer, String> {
    let items = entry.as_object().ok_or("not an object")?;

    let light_bounces = match items.get("light_bounces") {
        Some(v) => try_for!(v.decode(), "light_bounces"),
        None => 8,
    };

    let algorithm = Algorithm::Bidirectional(bidirectional::BidirParams {
        bounces: light_bounces,
    });

    decode_renderer(items, algorithm)
}

fn decode_photon_mapping(_path: &'_ Path, entry: Entry<'_>) -> Result<Renderer, String> {
    let items = entry.as_object().ok_or("not an object")?;

    let photons = match items.get("photons") {
        Some(v) => try_for!(v.decode(), "photons"),
        None => 10000,
    };

    let photon_bounces = match items.get("photon_bounces") {
        Some(v) => try_for!(v.decode(), "photon_bounces"),
        None => 8,
    };

    let photon_passes = match items.get("photon_passes") {
        Some(v) => try_for!(v.decode(), "photon_passes"),
        None => 1,
    };

    let radius = match items.get("radius") {
        Some(v) => try_for!(v.decode(), "radius"),
        None => 0.1,
    };

    let algorithm = Algorithm::PhotonMapping(photon_mapping::Config {
        photons: photons,
        photon_bounces: photon_bounces,
        photon_passes: photon_passes,
        radius: radius,
    });

    decode_renderer(items, algorithm)
}
