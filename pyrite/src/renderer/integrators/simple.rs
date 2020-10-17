//! Basic, backward tracing integrator.

use std::time::{Duration, Instant};

use bumpalo::Bump;
use cgmath::{InnerSpace, Point2, Point3, Vector3};
use collision::Ray3;
use rand::SeedableRng;
use rand_xorshift::XorShiftRng;

use crate::{
    cameras::Camera,
    film::{Film, Sample},
    lamp::Lamp,
    light::{Light, LightPool, Wavelengths},
    materials::{Material, MaterialInput},
    math::power_heuristic,
    program::{ExecutionContext, Resources},
    renderer::{
        algorithm::{make_tiles, Tile},
        LocalProgress, Progress, Renderer, TaskRunner,
    },
    shapes::Normal,
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
    let light_pool = LightPool::new(&arena, renderer.spectrum_bins);
    let mut wavelengths = Wavelengths::new(renderer.spectrum_samples);

    let message = format!("Tile {}", index + 1);
    progress.show(&message, tile.area() as u64);
    let mut last_progress = Instant::now();

    let mut tools = Tools {
        sampler: &mut rng,
        light_pool: &light_pool,
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
                        .zip(&throughput)
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

                let interaction = material.sample_reflection(
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

                if interaction.reflectivity.is_black() || interaction.pdf == 0.0 {
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

                throughput *= interaction.reflectivity
                    * interaction.in_direction.dot(shading_normal_vector).abs()
                    / interaction.pdf;
                specular_bounce = !interaction.diffuse;

                if bounce > 3 {
                    let rr_probability = (1.0 - throughput.max()).max(0.05);

                    if tools.sampler.gen_f32() < rr_probability {
                        break;
                    }

                    throughput *= 1.0 / (1.0 - rr_probability);
                }

                ray = Ray3::new(
                    intersection.surface_point.position,
                    interaction.in_direction,
                );
            }

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

    progress.set_progress(tile.area() as u64 - 1);
}

fn sample_light<'t, 'a>(
    world: &'a World<'a>,
    hit: &Hit<'a>,
    wavelengths: &Wavelengths,
    tools: &mut Tools<'t, 'a>,
) -> Option<Light<'t>> {
    let lamps = &world.lights[..];
    let num_lamps = lamps.len();
    let lamp = tools.sampler.select(lamps)?;

    let mut light = estimate_direct(world, hit, lamp, wavelengths, tools);
    light *= num_lamps as f32;

    Some(light)
}

fn estimate_direct<'t, 'a>(
    world: &'a World<'a>,
    hit: &Hit<'a>,
    lamp: &Lamp<'a>,
    wavelengths: &Wavelengths,
    tools: &mut Tools<'t, 'a>,
) -> Light<'t> {
    let material_input = MaterialInput {
        wavelength: wavelengths.hero(),
        wavelength_used: false.into(),
        normal: hit.normal.vector(),
        ray_direction: -hit.out_direction,
        texture_coordinate: hit.texture_coordinate,
    };

    let mut sample = lamp.sample_emission(world, hit.position, wavelengths, tools);

    let mut reflected_light = tools.light_pool.get();

    if sample.pdf > 0.0 && !sample.light.is_black() {
        let mut reflection = hit.bsdf.evaluate(
            hit.out_direction,
            hit.normal,
            sample.in_direction,
            hit.texture_coordinate,
            wavelengths,
            tools,
        ) * sample.in_direction.dot(hit.normal.vector()).abs();

        let scattering_pdf = hit.bsdf.pdf(
            hit.out_direction,
            hit.normal,
            sample.in_direction,
            &material_input,
            tools.execution_context,
        );

        if material_input.wavelength_used.get() {
            reflection.set_single_wavelength();
        }

        if !reflection.is_black() {
            if !sample.visible {
                sample.light.set_all(0.0);
            }

            if !sample.light.is_black() {
                if matches!(lamp, &Lamp::Shape(_)) {
                    let weight = power_heuristic(1.0, sample.pdf, 1.0, scattering_pdf);
                    reflected_light += reflection * sample.light * weight / sample.pdf;
                } else {
                    reflected_light += reflection * sample.light / sample.pdf;
                }
            }
        }
    }

    // BSDF multiple importance sampling
    if let &Lamp::Shape(lamp_shape) = lamp {
        let mut interaction = hit
            .bsdf
            .sample_reflection(
                hit.out_direction,
                hit.texture_coordinate,
                hit.normal,
                wavelengths,
                tools,
            )
            .expect("the path should have terminated before direct light sampling");
        let scattering_pdf = interaction.pdf;

        interaction.reflectivity *= interaction.in_direction.dot(hit.normal.vector()).abs();

        if !interaction.reflectivity.is_black() && scattering_pdf > 0.0 {
            let weight = if interaction.diffuse {
                let light_pdf =
                    lamp_shape.emission_pdf(hit.position, interaction.in_direction, sample.normal);
                if light_pdf == 0.0 {
                    return reflected_light;
                }

                power_heuristic(1.0, scattering_pdf, 1.0, light_pdf)
            } else {
                1.0
            };

            let ray = Ray3::new(hit.position, interaction.in_direction);
            let intersection = world.intersect(ray);

            let light_contribution = if let Some(intersection) = intersection {
                if intersection.surface_point.is_shape(lamp_shape) {
                    let surface_data = intersection.surface_point.get_surface_data();
                    let material = intersection.surface_point.get_material();

                    let input = NormalInput {
                        normal: surface_data.normal.vector(),
                        incident: ray.direction,
                        texture: surface_data.texture,
                    };
                    let shading_normal = material.apply_normal_map(
                        surface_data.normal,
                        input,
                        tools.execution_context,
                    );

                    material.light_emission(
                        -interaction.in_direction,
                        shading_normal,
                        surface_data.texture,
                        wavelengths,
                        tools,
                    )
                } else {
                    None
                }
            } else {
                // Some(sample.light_emission(ray))
                None
            };

            if let Some(light_contribution) = light_contribution {
                if !light_contribution.is_black() {
                    reflected_light +=
                        interaction.reflectivity * light_contribution * weight / scattering_pdf;
                }
            }
        }
    }

    reflected_light
}

struct Hit<'a> {
    position: Point3<f32>,
    out_direction: Vector3<f32>,
    normal: Normal,
    texture_coordinate: Point2<f32>,
    bsdf: Material<'a>,
}
