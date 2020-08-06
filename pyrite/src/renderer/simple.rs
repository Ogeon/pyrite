use rand::{Rng, SeedableRng};
use rand_xorshift::XorShiftRng;

use super::{
    algorithm::{contribute, make_tiles, Tile},
    LocalProgress, Progress, Renderer, TaskRunner,
};
use crate::cameras::Camera;
use crate::film::{Film, Sample};
use crate::tracer::trace;
use crate::{
    program::{ExecutionContext, Resources},
    world::World,
};
use std::time::{Duration, Instant};

pub(crate) fn render<F: FnMut(Progress<'_>)>(
    film: &Film,
    task_runner: TaskRunner,
    mut on_status: F,
    renderer: &Renderer,
    world: &World,
    camera: &Camera,
    resources: Resources,
) {
    fn gen_rng() -> XorShiftRng {
        XorShiftRng::from_rng(rand::thread_rng()).expect("could not generate RNG")
    }

    let status_message = "Rendering";
    on_status(Progress {
        progress: 0,
        message: &status_message,
    });

    let tiles = make_tiles(film.width(), film.height(), renderer.tile_size, camera);

    let mut progress: usize = 0;
    let num_tiles = tiles.len();

    task_runner.run_tasks(
        tiles.into_iter().map(|f| (f, gen_rng())),
        |index, (tile, rng), progress| {
            render_tile(
                index, rng, tile, film, camera, world, resources, renderer, progress,
            );
        },
        |_, _| {
            progress += 1;
            on_status(Progress {
                progress: ((progress * 100) / num_tiles) as u8,
                message: &status_message,
            });
        },
    );
}

fn render_tile<R: Rng>(
    index: usize,
    mut rng: R,
    tile: Tile,
    film: &Film,
    camera: &Camera,
    world: &World,
    resources: Resources,
    renderer: &Renderer,
    progress: LocalProgress,
) {
    let mut additional_samples = Vec::with_capacity(renderer.spectrum_samples as usize - 1);
    let mut path = Vec::with_capacity(renderer.bounces as usize);
    let mut exe = ExecutionContext::new(resources);

    let iterations = tile.area() as u64 * renderer.pixel_samples as u64;
    let message = format!("Tile {}", index + 1);
    let mut last_progress = Instant::now();
    progress.show(&message, iterations);

    for i in 0..iterations {
        if Instant::now() - last_progress > Duration::from_millis(100) {
            progress.set_progress(i);
            last_progress = Instant::now();
        }

        additional_samples.clear();
        path.clear();

        let position = tile.sample_point(&mut rng);

        let ray = camera.ray_towards(&position, &mut rng);

        additional_samples.extend(
            film.sample_many_wavelengths(&mut rng, renderer.spectrum_samples as usize)
                .map(|wavelength| {
                    (
                        Sample {
                            wavelength,
                            brightness: 0.0,
                            weight: 1.0,
                        },
                        1.0,
                    )
                }),
        );

        let mut main_sample =
            additional_samples.swap_remove(rng.gen_range(0, additional_samples.len()));
        let wavelength = main_sample.0.wavelength;

        trace(
            &mut path,
            &mut rng,
            ray,
            wavelength,
            world,
            renderer.bounces,
            renderer.light_samples,
            &mut exe,
        );

        let mut used_additional = true;

        for bounce in &path {
            for &mut (ref mut sample, ref mut reflectance) in &mut additional_samples {
                used_additional =
                    contribute(bounce, sample, reflectance, true, &mut exe) && used_additional;
            }

            let (ref mut sample, ref mut reflectance) = main_sample;
            contribute(bounce, sample, reflectance, false, &mut exe);
        }

        film.expose(position, main_sample.0);

        if used_additional {
            for (sample, _) in additional_samples.drain(..) {
                film.expose(position, sample);
            }
        }
    }
}
