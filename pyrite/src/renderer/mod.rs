use std::{error::Error, path::PathBuf};

use num_cpus;

use crate::cameras;
use crate::world;

use crate::film::Film;
use crate::project::FromExpression;

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
    pub fn from_project(
        project: crate::project::Renderer,
        make_path: &impl Fn(&str) -> PathBuf,
    ) -> Result<Self, Box<dyn Error>> {
        match project {
            crate::project::Renderer::Simple { shared } => {
                Self::from_shared(shared, Algorithm::Simple, make_path)
            }
            crate::project::Renderer::Bidirectional {
                shared,
                light_bounces,
            } => Self::from_shared(
                shared,
                Algorithm::Bidirectional(bidirectional::BidirParams {
                    bounces: u32::from_expression_or(light_bounces, make_path, 8)?,
                }),
                make_path,
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
                    radius: f32::from_expression_or(radius, make_path, 0.1)?,
                    photon_bounces: u32::from_expression_or(photon_bounces, make_path, 8)?,
                    photons: usize::from_expression_or(photons, make_path, 10000)?,
                    photon_passes: usize::from_expression_or(photon_passes, make_path, 1)?,
                }),
                make_path,
            ),
        }
    }

    fn from_shared(
        shared: crate::project::RendererShared,
        algorithm: Algorithm,
        make_path: &impl Fn(&str) -> PathBuf,
    ) -> Result<Self, Box<dyn Error>> {
        Ok(Self {
            threads: usize::from_expression_or_else(shared.threads, make_path, || num_cpus::get())?,
            bounces: u32::from_expression_or(shared.bounces, make_path, 8)?,
            pixel_samples: shared.pixel_samples.parse(make_path)?,
            light_samples: usize::from_expression_or(shared.light_samples, make_path, 4)?,
            spectrum_samples: u32::from_expression_or(shared.spectrum_samples, make_path, 10)?,
            spectrum_bins: usize::from_expression_or(shared.spectrum_resolution, make_path, 64)?,
            spectrum_span: DEFAULT_SPECTRUM_SPAN,
            tile_size: usize::from_expression_or(shared.tile_size, make_path, 32)?,
            algorithm,
        })
    }

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
