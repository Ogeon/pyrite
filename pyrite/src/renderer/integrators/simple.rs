//! Basic, backward tracing integrator.

use std::time::{Duration, Instant};

use bumpalo::Bump;
use cgmath::{InnerSpace, Point2};
use collision::Ray3;
use rand::SeedableRng;
use rand_xorshift::XorShiftRng;

use crate::{
    cameras::Camera,
    film::{Film, Sample},
    light::{LightPool, Wavelengths},
    materials::InteractionOutput,
    pooling::SlicePool,
    program::{ExecutionContext, Resources},
    renderer::{
        algorithm::{make_tiles, Tile},
        sample_light::{sample_light, Hit},
        LocalProgress, Progress, Renderer, TaskRunner,
    },
    tracer::{NormalInput, RenderContext},
    utils::Tools,
    world::World,
};

pub(crate) struct Config {
    pub direct_light: bool,
}

pub(crate) fn render<F: FnMut(Progress<'_>)>(
    film: &Film,
    task_runner: TaskRunner,
    mut on_status: F,
    renderer: &Renderer,
    config: &Config,
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
                index, rng, tile, film, camera, world, resources, renderer, config, progress,
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

pub(crate) fn render_tile(
    index: usize,
    mut rng: XorShiftRng,
    tile: Tile,
    film: &Film,
    camera: &Camera,
    world: &World,
    resources: Resources,
    renderer: &Renderer,
    config: &Config,
    progress: LocalProgress,
) {
    let mut exe = ExecutionContext::new(resources);
    let arena = Bump::new();
    let light_pool = LightPool::new(&arena, renderer.spectrum_samples);
    let interaction_output_pool = SlicePool::new(&arena, renderer.spectrum_samples);
    let mut wavelengths = Wavelengths::new(renderer.spectrum_samples);

    let message = format!("Tile {}", index + 1);
    progress.show(&message, tile.area() as u64);
    let mut last_progress = Instant::now();

    let mut tools = Tools {
        sampler: &mut rng,
        light_pool: &light_pool,
        interaction_output_pool: &interaction_output_pool,
        execution_context: &mut exe,
    };

    for area_iter in 0..(tile.area() as u64) {
        if Instant::now() - last_progress > Duration::from_millis(1000) {
            progress.set_progress(area_iter);
            last_progress = Instant::now();
        }

        for _ in 0..renderer.pixel_samples {
            wavelengths.sample(film, tools.sampler);

            let pixel_position = tile.sample_point(tools.sampler);

            let mut accumulated_light = light_pool.get();
            let mut throughput = light_pool.with_value(1.0);

            let mut ray = camera.ray_towards(pixel_position, tools.sampler);
            let mut specular_bounce = false;
            let mut dispersed = false;

            for bounce in 0..renderer.bounces {
                let intersection = if let Some(intersection) = world.intersect(ray) {
                    intersection
                } else {
                    let input = RenderContext {
                        wavelength: wavelengths.hero(),
                        normal: -ray.direction,
                        ray_direction: ray.direction,
                        texture: Point2::new(0.0, 0.0),
                    };
                    let mut color_program = world.sky.memoize(input, tools.execution_context);

                    for ((bin, throughput), wavelength) in accumulated_light
                        .iter_mut()
                        .zip(&*throughput)
                        .zip(&wavelengths)
                    {
                        color_program.update_input().set_wavelength(wavelength);
                        *bin += throughput * color_program.run();
                    }

                    break;
                };

                let surface_data = intersection.surface_point.get_surface_data();
                let material = intersection.surface_point.get_material();

                let input = NormalInput {
                    normal: surface_data.normal.vector(),
                    incident: ray.direction,
                    texture: surface_data.texture,
                };
                let shading_normal_vector =
                    material.apply_normal_map(surface_data.normal, input, tools.execution_context);
                let shading_normal = surface_data.normal.tilted(shading_normal_vector);

                if bounce == 0 || specular_bounce || !config.direct_light {
                    if let Some(emission) = material.light_emission(
                        -ray.direction,
                        shading_normal_vector,
                        surface_data.texture,
                        &wavelengths,
                        &mut tools,
                    ) {
                        accumulated_light += emission * &throughput;
                    }
                }

                let interaction = material.sample_reflection_coherent(
                    -ray.direction,
                    surface_data.texture,
                    shading_normal,
                    &wavelengths,
                    &mut tools,
                );

                let interaction = if let Some(interaction) = interaction {
                    interaction
                } else {
                    break; // Emissive only, so terminate
                };

                let (reflectivity, pdf, in_direction) = match interaction.output {
                    InteractionOutput::Coherent(output) => {
                        (output.reflectivity, output.pdf, output.in_direction)
                    }
                    InteractionOutput::Dispersed(output) => {
                        let output = output[0];
                        let mut reflectivity = tools.light_pool.get();
                        reflectivity[0] = output.reflectivity.value();
                        dispersed = true;
                        (reflectivity, output.pdf, output.in_direction)
                    }
                };

                if reflectivity.is_black() || pdf == 0.0 {
                    break;
                }

                if config.direct_light {
                    let hit = Hit {
                        position: intersection.surface_point.position,
                        out_direction: -ray.direction,
                        normal: shading_normal,
                        texture_coordinate: surface_data.texture,
                        bsdf: material,
                    };

                    if let Some(light) = sample_light(world, &hit, &wavelengths, &mut tools) {
                        accumulated_light += light * &throughput;
                    }
                }

                throughput *= reflectivity * in_direction.dot(shading_normal_vector).abs() / pdf;
                specular_bounce = !interaction.diffuse;

                if bounce > 3 {
                    let rr_probability = (1.0 - throughput.max()).max(0.05);

                    if tools.sampler.gen_f32() < rr_probability {
                        break;
                    }

                    throughput *= 1.0 / (1.0 - rr_probability);
                }

                ray = Ray3::new(intersection.surface_point.position, in_direction);
            }

            if dispersed {
                film.expose(
                    pixel_position,
                    Sample {
                        brightness: accumulated_light[0],
                        wavelength: wavelengths[0],
                        weight: 1.0,
                    },
                )
            } else {
                for (&brightness, wavelength) in accumulated_light.iter().zip(&wavelengths) {
                    film.expose(
                        pixel_position,
                        Sample {
                            brightness,
                            wavelength,
                            weight: 1.0,
                        },
                    )
                }
            }
        }
    }

    progress.set_progress(tile.area() as u64 - 1);
}
