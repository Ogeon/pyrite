use rand::{Rng, SeedableRng};
use rand_xorshift::XorShiftRng;

use cameras::Camera;
use film::{Film, Sample, Tile};
use renderer::algorithm::contribute;
use renderer::{Renderer, Status, WorkPool};
use tracer::{trace, Light};
use world::World;

pub fn render<W: WorkPool, F: FnMut(Status)>(
    film: &Film,
    workers: &mut W,
    mut on_status: F,
    renderer: &Renderer,
    world: &World<XorShiftRng>,
    camera: &Camera,
) {
    fn gen_rng() -> XorShiftRng {
        XorShiftRng::from_rng(rand::thread_rng()).expect("could not generate RNG")
    }

    let status_message = "rendering";
    on_status(Status {
        progress: 0,
        message: &status_message,
    });

    let mut progress: usize = 0;
    let num_tiles = film.num_tiles();

    workers.do_work(
        film.into_iter().map(|f| (f, gen_rng())),
        |(mut tile, rng)| {
            render_tile(rng, &mut tile, camera, world, renderer);
        },
        |_, _| {
            progress += 1;
            on_status(Status {
                progress: ((progress * 100) / num_tiles) as u8,
                message: &status_message,
            });
        },
    );
}

fn render_tile<R: Rng>(
    mut rng: R,
    tile: &mut Tile,
    camera: &Camera,
    world: &World<R>,
    renderer: &Renderer,
) {
    for _ in 0..(tile.area() * renderer.pixel_samples as usize) {
        let position = tile.sample_point(&mut rng);

        let ray = camera.ray_towards(&position, &mut rng);
        let wavelength = tile.sample_wavelength(&mut rng);
        let light = Light::new(wavelength);
        let path = trace(
            &mut rng,
            ray,
            light,
            world,
            renderer.bounces,
            renderer.light_samples,
        );

        let mut main_sample = (
            Sample {
                wavelength: wavelength,
                brightness: 0.0,
                weight: 1.0,
            },
            1.0,
        );

        let mut used_additional = true;
        let mut additional_samples: Vec<_> = (0..renderer.spectrum_samples - 1)
            .map(|_| {
                (
                    Sample {
                        wavelength: tile.sample_wavelength(&mut rng),
                        brightness: 0.0,
                        weight: 1.0,
                    },
                    1.0,
                )
            })
            .collect();

        for bounce in &path {
            for &mut (ref mut sample, ref mut reflectance) in &mut additional_samples {
                used_additional = contribute(bounce, sample, reflectance, true) && used_additional;
            }

            let (ref mut sample, ref mut reflectance) = main_sample;
            contribute(bounce, sample, reflectance, false);
        }

        tile.expose(position, main_sample.0);

        if used_additional {
            for (sample, _) in additional_samples {
                tile.expose(position, sample);
            }
        }
    }
}
