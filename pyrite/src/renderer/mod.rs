use std::borrow::Cow;

use num_cpus;

use crate::cameras;
use crate::world;

use crate::{film::Film, program::Resources};
use indicatif::ProgressBar;

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

    pub(crate) fn render<F: FnMut(Progress<'_>)>(
        &self,
        film: &Film,
        task_runner: TaskRunner,
        on_status: F,
        camera: &cameras::Camera,
        world: &world::World,
        resources: Resources,
    ) {
        match self.algorithm {
            Algorithm::Simple => {
                simple::render(film, task_runner, on_status, self, world, camera, resources)
            }
            Algorithm::Bidirectional(ref config) => bidirectional::render(
                film,
                task_runner,
                on_status,
                self,
                config,
                world,
                camera,
                resources,
            ),
            Algorithm::PhotonMapping(ref config) => photon_mapping::render(
                film,
                task_runner,
                on_status,
                self,
                config,
                world,
                camera,
                resources,
            ),
        }
    }
}

pub enum Algorithm {
    Simple,
    Bidirectional(bidirectional::BidirParams),
    PhotonMapping(photon_mapping::Config),
}

pub(crate) struct TaskRunner {
    pub(crate) threads: usize,
    pub(crate) progress: ProgressIndicator,
}

impl TaskRunner {
    fn run_tasks<I, F, R, T>(&self, tasks: I, do_work: F, mut with_result: R)
    where
        I: IntoIterator + Send,
        I::Item: Send,
        F: Fn(usize, I::Item, LocalProgress) -> T + Send + Sync,
        R: FnMut(usize, T),
        T: Send,
    {
        crossbeam::thread::scope(|scope| {
            let (result_receiver, sender_receiver) = {
                let (sender_sender, sender_receiver) = crossbeam::channel::bounded(self.threads);
                let (result_sender, result_receiver) = crossbeam::channel::unbounded();

                for thread_id in 0..self.threads {
                    let sender_sender = sender_sender.clone();
                    let result_sender = result_sender.clone();
                    let do_work = &do_work;
                    let progress = &self.progress;

                    scope.spawn(move |_| {
                        let (task_sender, task_receiver) = crossbeam::channel::bounded(1);
                        if let Err(_) = sender_sender.send(task_sender.clone()) {
                            return;
                        }

                        while let Ok(Message::Task(index, task)) = task_receiver.recv() {
                            let result =
                                do_work(index, task, progress.get_local_progress_bar(thread_id));

                            if let Err(_) = result_sender.send((index, result)) {
                                return;
                            }

                            if let Err(_) = sender_sender.send(task_sender.clone()) {
                                return;
                            }
                        }
                    });
                }

                (result_receiver, sender_receiver)
            };

            scope.spawn(move |_| {
                for (index, task) in tasks.into_iter().enumerate() {
                    if let Ok(sender) = sender_receiver.recv() {
                        sender.send(Message::Task(index, task)).unwrap(); // workers should never close the channel
                    }
                }

                for sender in sender_receiver {
                    sender.send(Message::Stop).unwrap(); // workers should never close the channel
                }
            });

            for (index, result) in result_receiver {
                with_result(index, result);
            }
        })
        .unwrap();

        self.progress.clear_local_progress();
    }
}

enum Message<T> {
    Task(usize, T),
    Stop,
}

pub(crate) struct ProgressIndicator {
    pub bars: Vec<ProgressBar>,
}

impl ProgressIndicator {
    pub fn get_local_progress_bar(&self, id: usize) -> LocalProgress {
        LocalProgress {
            bar: self.bars[id].clone(),
        }
    }

    pub fn clear_local_progress(&self) {
        for bar in &self.bars {
            bar.finish_and_clear();
        }
    }
}

pub(crate) struct LocalProgress {
    bar: ProgressBar,
}

impl LocalProgress {
    pub fn show(&self, message: impl Into<Cow<'static, str>>, length: u64) {
        self.bar.set_message(message);
        self.bar.set_length(length);
    }

    pub fn set_progress(&self, progress: u64) {
        self.bar.set_position(progress);
    }
}

pub(crate) struct Progress<'a> {
    pub(crate) progress: u8,
    pub(crate) message: &'a str,
}
