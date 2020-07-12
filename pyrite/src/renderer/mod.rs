use num_cpus;

use crate::cameras;
use crate::world;

use crate::{film::Film, project::program::Resources};

mod algorithm;
mod bidirectional;
mod photon_mapping;
mod simple;

static DEFAULT_SPECTRUM_SPAN: (f32, f32) = (380.0, 780.0);

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
    pub fn from_project(project: crate::project::Renderer) -> Self {
        match project {
            crate::project::Renderer::Simple { shared } => {
                Self::from_shared(shared, Algorithm::Simple)
            }
            crate::project::Renderer::Bidirectional {
                shared,
                light_bounces,
            } => Self::from_shared(
                shared,
                Algorithm::Bidirectional(bidirectional::BidirParams {
                    bounces: light_bounces.unwrap_or(8),
                }),
            ),
            crate::project::Renderer::PhotonMapping {
                shared,
                radius,
                photons,
                photon_bounces,
                photon_passes,
            } => Self::from_shared(
                shared,
                Algorithm::PhotonMapping(photon_mapping::Config {
                    radius: radius.unwrap_or(0.1),
                    photon_bounces: photon_bounces.unwrap_or(8),
                    photons: photons.unwrap_or(10000),
                    photon_passes: photon_passes.unwrap_or(1),
                }),
            ),
        }
    }

    fn from_shared(shared: crate::project::RendererShared, algorithm: Algorithm) -> Self {
        Self {
            threads: shared.threads.unwrap_or_else(|| num_cpus::get()),
            bounces: shared.bounces.unwrap_or(8),
            pixel_samples: shared.pixel_samples,
            light_samples: shared.light_samples.unwrap_or(4),
            spectrum_samples: shared.spectrum_samples.unwrap_or(10),
            spectrum_bins: shared.spectrum_resolution.unwrap_or(64),
            spectrum_span: DEFAULT_SPECTRUM_SPAN,
            tile_size: shared.tile_size.unwrap_or(32),
            algorithm,
        }
    }

    pub(crate) fn render<W: WorkPool, F: FnMut(Status<'_>)>(
        &self,
        film: &Film,
        workers: &mut W,
        on_status: F,
        camera: &cameras::Camera,
        world: &world::World,
        resources: Resources,
    ) {
        match self.algorithm {
            Algorithm::Simple => {
                simple::render(film, workers, on_status, self, world, camera, resources)
            }
            Algorithm::Bidirectional(ref config) => bidirectional::render(
                film, workers, on_status, self, config, world, camera, resources,
            ),
            Algorithm::PhotonMapping(ref config) => photon_mapping::render(
                film, workers, on_status, self, config, world, camera, resources,
            ),
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
