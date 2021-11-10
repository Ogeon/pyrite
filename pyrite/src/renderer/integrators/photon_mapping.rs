use std::time::{Duration, Instant};

use bumpalo::Bump;
use cgmath::{InnerSpace, MetricSpace, Point2, Point3, Vector3};
use collision::{Aabb3, Ray3};
use colosseum::sync::Arena as LockedArena;
use crossbeam::atomic::AtomicCell;
use fixedbitset::FixedBitSet;
use rand::SeedableRng;
use rand_xorshift::XorShiftRng;

use crate::{
    cameras::Camera,
    film::Film,
    light::{CoherentLight, LightPool, Wavelengths},
    materials::{InteractionOutput, Material},
    pooling::{PooledSlice, SlicePool},
    program::{ExecutionContext, Resources},
    renderer::{
        algorithm::{make_tiles, Tile},
        sample_light::{sample_light, Hit},
        Progress, Renderer, TaskRunner, Worker,
    },
    shapes::{Normal, SurfacePoint},
    spatial::bvh::{Bounded, Bvh},
    tracer::{NormalInput, RenderContext},
    utils::{self, AtomicF32, Tools},
    world::World,
};

type LockedSlicePool<'a, T> = SlicePool<'a, T, utils::Mutex, LockedArena<T>>;
type VisiblePixels<'r, 'a> = Bvh<(&'r Pixel<'a>, VisiblePoint<'a>, LightMode<(), usize>)>;

const PHOTON_BATCH_SIZE: u64 = 8192;
const WRITE_FREQUENCY: u64 = 5;

pub(crate) struct Config {
    pub initial_radius: f32,
    pub iterations: u64,
    pub photons: u64,
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

    let max_tile_area = renderer.tile_size * renderer.tile_size;
    let tiles = make_tiles(film.width(), film.height(), renderer.tile_size, camera);
    let arena = Bump::new();
    let locked_arena = LockedArena::new();
    let visible_points_pool =
        SlicePool::<_, utils::Mutex, _>::new(&locked_arena, renderer.spectrum_bins);
    let mut wavelengths = Wavelengths::new(renderer.spectrum_bins);

    let mut pixels: Vec<_> = std::iter::repeat_with(|| {
        Pixel::new(
            config.initial_radius,
            renderer.spectrum_bins,
            renderer.spectrum_bins,
            &arena,
        )
    })
    .take(tiles.len() * max_tile_area) //Adding some overhead to make chunking easy
    .collect();

    let mut render_workers = make_render_workers(
        &task_runner,
        renderer,
        &visible_points_pool,
        gen_rng,
        camera,
        film,
        world,
        resources,
        config,
    );

    let mut bvh_workers = std::iter::repeat(BvhWorker)
        .take(task_runner.threads)
        .collect();

    on_status(Progress {
        message: "Iteration 1",
        progress: 0,
    });
    let mut last_progress = Instant::now();

    for iteration in 0..config.iterations {
        let iterations = iteration + 1;

        if Instant::now() - last_progress > Duration::from_millis(1000) {
            on_status(Progress {
                message: &format!("Iteration {}", iterations),
                progress: ((iteration * 100) / config.iterations) as u8,
            });
            last_progress = Instant::now();
        }

        wavelengths.sample(film, &mut gen_rng());

        // Trace from camera
        {
            let tiles = &tiles;
            let pixels = &mut pixels;
            let wavelengths = &wavelengths;
            let (workers, _) = task_runner.scoped_pool(
                render_workers,
                tiles
                    .iter()
                    .zip(pixels.chunks_exact_mut(max_tile_area))
                    .enumerate()
                    .map(move |(tile_index, (tile, pixels))| {
                        RenderTask::TraceTile(tile_index, tile, pixels, wavelengths)
                    }),
            );

            render_workers = workers;
        }

        // Trace photons
        {
            let wavelengths = &wavelengths;

            let chunk_size = (pixels.len() as f64 / task_runner.threads as f64).ceil() as usize;
            let pixel_chunks = pixels.chunks(chunk_size);

            //let begin_building = Instant::now();
            let (workers, pixels) = task_runner.scoped_pool(bvh_workers, pixel_chunks);
            bvh_workers = workers;
            /*let diff = (Instant::now() - begin_building).as_millis() as f64 / 1000.0;
            task_runner.progress.bars[1]
                .finish_with_message(&format!("Pixel BVH built in {} seconds", diff));*/

            let initial_batch = if config.photons > PHOTON_BATCH_SIZE {
                (PHOTON_BATCH_SIZE, config.photons - PHOTON_BATCH_SIZE)
            } else {
                (config.photons, 0)
            };

            let batches = std::iter::successors(Some(initial_batch), |&(_, remaining)| {
                if remaining == 0 {
                    None
                } else if remaining > PHOTON_BATCH_SIZE {
                    Some((PHOTON_BATCH_SIZE, remaining - PHOTON_BATCH_SIZE))
                } else {
                    Some((remaining, 0))
                }
            })
            .enumerate()
            .map(|(index, (photons, _))| {
                RenderTask::TracePhotons(index, photons, &pixels, wavelengths)
            });

            let (workers, _) = task_runner.scoped_pool(render_workers, batches);
            render_workers = workers;
        }

        // Gather contributions
        for pixel in &mut pixels {
            let m = pixel.m.load() as f32 / renderer.spectrum_bins as f32;
            if m > 0.0 {
                let gamma = 2.0 / 3.0;
                let new_n = pixel.n + gamma * m;
                let new_radius = pixel.radius * (new_n / (pixel.n + m)).sqrt();

                let radius_ratio = new_radius * new_radius / (pixel.radius * pixel.radius);
                let bins = pixel.throughput.iter().zip(pixel.phi).zip(&wavelengths);
                for ((&throughput, phi), wavelength) in bins {
                    let index = film.wavelength_to_grain(wavelength);
                    let tau = &mut pixel.tau[index];
                    *tau = (phi.load() * throughput + *tau) * radius_ratio;
                }

                pixel.n = new_n;
                pixel.radius = new_radius;

                pixel.m.store(0);
                for phi in pixel.phi {
                    phi.store(0.0);
                }
            }

            pixel.throughput.iter_mut().for_each(|bin| *bin = 0.0);
            pixel.visible_point = LightMode::Coherent(None);
        }

        // Write to film
        if iterations == config.iterations || (iterations) % WRITE_FREQUENCY == 0 {
            let number_of_photons = (iterations) * config.photons;

            {
                let tiles = &tiles;
                let pixels = &mut pixels;
                let (workers, _) = task_runner.scoped_pool(
                    render_workers,
                    tiles
                        .iter()
                        .zip(pixels.chunks_exact_mut(max_tile_area))
                        .map(|(tile, pixels)| RenderTask::WriteToFilm {
                            tile,
                            pixels,
                            iterations,
                            number_of_photons,
                        }),
                );

                render_workers = workers;
            }
        }
    }

    on_status(Progress {
        message: &format!("Iteration {}", config.iterations),
        progress: 100,
    });
}

fn make_render_workers<'a>(
    task_runner: &TaskRunner,
    renderer: &'a Renderer,
    visible_points_pool: &'a LockedSlicePool<'a, Option<VisiblePoint<'a>>>,
    gen_rng: fn() -> XorShiftRng,
    camera: &'a Camera,
    film: &'a Film,
    world: &'a World,
    resources: Resources<'a>,
    config: &'a Config,
) -> Vec<RenderWorker<'a>> {
    let mut render_workers = Vec::with_capacity(task_runner.pool_size());
    for _ in 0..task_runner.pool_size() {
        let rng = gen_rng();
        let arena = Bump::new();
        let light_pool = LightPool::<utils::RefCell>::new(&arena, renderer.spectrum_bins);

        render_workers.push(RenderWorker {
            visible_points_pool,
            rng,
            camera,
            film,
            world,
            resources,
            renderer,
            config,
        });
    }
    render_workers
}

fn trace_tile<'a>(
    tile_index: usize,
    tile: &Tile,
    pixels: &mut [Pixel<'a>],
    visible_points_pool: &'a LockedSlicePool<'a, Option<VisiblePoint<'a>>>,
    rng: &mut XorShiftRng,
    camera: &Camera,
    film: &Film,
    world: &'a World,
    resources: Resources,
    renderer: &Renderer,
    config: &Config,
    wavelengths: &Wavelengths,
) {
    let mut exe = ExecutionContext::new(resources);
    let arena = Bump::new();
    let light_pool = LightPool::new(&arena, renderer.spectrum_bins);
    let interaction_output_pool = SlicePool::new(&arena, renderer.spectrum_bins);

    let mut tools = Tools {
        sampler: rng,
        light_pool: &light_pool,
        interaction_output_pool: &interaction_output_pool,
        execution_context: &mut exe,
    };

    let mut rays = Vec::with_capacity(renderer.spectrum_bins);
    let mut intersections = Vec::with_capacity(renderer.spectrum_bins);
    let mut interactions = Vec::with_capacity(renderer.spectrum_bins);

    //let message = format!("Tile {}", index + 1);
    //progress.show(&message, tile.area() as u64);
    //let mut last_progress = Instant::now();

    for (area_iter, (pixel_area, pixel)) in tile.pixels().zip(pixels).enumerate() {
        //if Instant::now() - last_progress > Duration::from_millis(1000) {
        //    progress.set_progress(area_iter as u64);
        //    last_progress = Instant::now();
        //}

        let pixel_position = pixel_area.sample_point(tools.sampler);
        pixel.pixel_position = pixel_position;
        let mut next_ray: LightMode<_, &[_]> =
            LightMode::Coherent(camera.ray_towards(pixel_position, tools.sampler));

        let mut throughput = light_pool.with_value(1.0);
        let mut specular_bounce = FixedBitSet::with_capacity(renderer.spectrum_bins);
        for bounce in 0..renderer.bounces {
            let intersection = match next_ray {
                LightMode::Coherent(ray) => {
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

                        for ((bin, throughput), wavelength) in pixel
                            .accumulated_light
                            .iter_mut()
                            .zip(&*throughput)
                            .zip(wavelengths)
                        {
                            color_program.update_input().set_wavelength(wavelength);
                            *bin += throughput * color_program.run();
                        }

                        break;
                    };

                    LightMode::Coherent((intersection, ray))
                }
                LightMode::Dispersed(rays) => {
                    let new_intersections = rays.iter().filter_map(|&(index, ray)| {
                        let intersection = world.intersect(ray);

                        if intersection.is_none() {
                            let input = RenderContext {
                                wavelength: wavelengths[index],
                                normal: -ray.direction,
                                ray_direction: ray.direction,
                                texture: Point2::new(0.0, 0.0),
                            };
                            pixel.accumulated_light[index] +=
                                throughput[index] * tools.execution_context.run(world.sky, &input);
                        };

                        intersection.map(|intersection| (index, intersection, ray))
                    });

                    intersections.clear();
                    intersections.extend(new_intersections);

                    LightMode::Dispersed(&*intersections)
                }
            };

            let interaction = match intersection {
                LightMode::Coherent((intersection, ray)) => {
                    let surface_data = intersection.surface_point.get_surface_data();
                    let material = intersection.surface_point.get_material();

                    let input = NormalInput {
                        normal: surface_data.normal.vector(),
                        incident: ray.direction,
                        texture: surface_data.texture,
                    };
                    let shading_normal_vector = material.apply_normal_map(
                        surface_data.normal,
                        input,
                        tools.execution_context,
                    );
                    let shading_normal = surface_data.normal.tilted(shading_normal_vector);

                    if bounce == 0 || specular_bounce[0] || !config.direct_light {
                        if let Some(emission) = material.light_emission(
                            -ray.direction,
                            shading_normal_vector,
                            surface_data.texture,
                            &wavelengths,
                            &mut tools,
                        ) {
                            for (bin, &light) in pixel
                                .accumulated_light
                                .iter_mut()
                                .zip((emission * &throughput).iter())
                            {
                                *bin += light;
                            }
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

                    let diffuse = interaction.diffuse;
                    let glossy = interaction.glossy;
                    match interaction.output {
                        InteractionOutput::Coherent(output) => {
                            LightMode::Coherent(InteractionData {
                                output,
                                shading_normal_vector,
                                shading_normal,
                                material,
                                texture_coordinate: surface_data.texture,
                                ray,
                                surface_point: intersection.surface_point,
                                diffuse,
                                glossy,
                            })
                        }
                        InteractionOutput::Dispersed(output) => {
                            let new_interactions =
                                output.iter().enumerate().map(|(index, &output)| {
                                    let interaction = InteractionData {
                                        output,
                                        shading_normal_vector,
                                        shading_normal,
                                        material,
                                        texture_coordinate: surface_data.texture,
                                        ray,
                                        surface_point: intersection.surface_point,
                                        diffuse,
                                        glossy,
                                    };

                                    (index, interaction)
                                });

                            interactions.clear();
                            interactions.extend(new_interactions);

                            LightMode::Dispersed(&*interactions)
                        }
                    }
                }
                LightMode::Dispersed(intersections) => {
                    let new_interactions =
                        intersections
                            .iter()
                            .filter_map(|&(index, intersection, ray)| {
                                let surface_data = intersection.surface_point.get_surface_data();
                                let material = intersection.surface_point.get_material();

                                let input = NormalInput {
                                    normal: surface_data.normal.vector(),
                                    incident: ray.direction,
                                    texture: surface_data.texture,
                                };
                                let shading_normal_vector = material.apply_normal_map(
                                    surface_data.normal,
                                    input,
                                    tools.execution_context,
                                );
                                let shading_normal =
                                    surface_data.normal.tilted(shading_normal_vector);

                                if bounce == 0 || specular_bounce[index] || !config.direct_light {
                                    if let Some(emission) = material.light_emission(
                                        -ray.direction,
                                        shading_normal_vector,
                                        surface_data.texture,
                                        &wavelengths,
                                        &mut tools,
                                    ) {
                                        pixel.accumulated_light[index] +=
                                            (emission * &throughput)[index];
                                    }
                                }

                                let interaction = material.sample_reflection_dispersed(
                                    -ray.direction,
                                    surface_data.texture,
                                    shading_normal,
                                    index,
                                    &wavelengths,
                                    &mut tools,
                                );

                                interaction.map(|interaction| {
                                    let interaction = InteractionData {
                                        output: interaction.output,
                                        shading_normal_vector,
                                        shading_normal,
                                        material,
                                        ray,
                                        texture_coordinate: surface_data.texture,
                                        surface_point: intersection.surface_point,
                                        diffuse: interaction.diffuse,
                                        glossy: interaction.glossy,
                                    };
                                    (index, interaction)
                                })
                            });

                    interactions.clear();
                    interactions.extend(new_interactions);

                    LightMode::Dispersed(&*interactions)
                }
            };

            next_ray = match interaction {
                LightMode::Coherent(interaction_data) => {
                    let InteractionData {
                        output,
                        shading_normal_vector,
                        shading_normal,
                        material,
                        ray,
                        texture_coordinate,
                        surface_point,
                        diffuse,
                        glossy,
                    } = interaction_data;

                    if output.reflectivity.is_black() || output.pdf == 0.0 {
                        break;
                    }

                    if config.direct_light {
                        let hit = Hit {
                            position: surface_point.position,
                            out_direction: -ray.direction,
                            normal: shading_normal,
                            texture_coordinate,
                            bsdf: material,
                        };

                        if let Some(light) = sample_light(world, &hit, &wavelengths, &mut tools) {
                            for (bin, &light) in pixel
                                .accumulated_light
                                .iter_mut()
                                .zip((light * &throughput).iter())
                            {
                                *bin += light;
                            }
                        }
                    }

                    if diffuse || (glossy && bounce == renderer.bounces - 1) {
                        pixel.visible_point = LightMode::Coherent(Some(VisiblePoint {
                            position: surface_point.position,
                            out_direction: -ray.direction,
                            bsdf: material,
                        }));

                        pixel.throughput.copy_from_slice(&throughput);

                        break;
                    }

                    throughput *= output.reflectivity
                        * output.in_direction.dot(shading_normal_vector).abs()
                        / output.pdf;
                    specular_bounce.set(0, !diffuse);

                    if throughput.max() < 0.25 {
                        let continue_probability = throughput.max().min(1.0);

                        if tools.sampler.gen_f32() > continue_probability {
                            break;
                        }

                        throughput /= continue_probability;
                    }

                    LightMode::Coherent(Ray3::new(surface_point.position, output.in_direction))
                }
                LightMode::Dispersed(interactions) => {
                    if matches!(pixel.visible_point, LightMode::Coherent(_)) {
                        pixel.visible_point =
                            LightMode::Dispersed(visible_points_pool.get_fill_copy(None));
                    }

                    let visible_points =
                        if let LightMode::Dispersed(visible_points) = &mut pixel.visible_point {
                            visible_points
                        } else {
                            unreachable!("it should be dispersed at this point")
                        };

                    let pixel_throughput = &mut *pixel.throughput;
                    let accumulated_light = &mut *pixel.accumulated_light;

                    let new_rays =
                        interactions
                            .iter()
                            .filter_map(|&(index, ref interaction_data)| {
                                let InteractionData {
                                    output,
                                    shading_normal_vector,
                                    shading_normal,
                                    material,
                                    texture_coordinate,
                                    ray,
                                    surface_point,
                                    diffuse,
                                    glossy,
                                } = *interaction_data;

                                if output.reflectivity.value() == 0.0 || output.pdf == 0.0 {
                                    return None;
                                }

                                if config.direct_light {
                                    let hit = Hit {
                                        position: surface_point.position,
                                        out_direction: -ray.direction,
                                        normal: shading_normal,
                                        texture_coordinate,
                                        bsdf: material,
                                    };

                                    if let Some(light) =
                                        sample_light(world, &hit, &wavelengths, &mut tools)
                                    {
                                        accumulated_light[index] +=
                                            light[index] * &throughput[index];
                                    }
                                }

                                if diffuse || (glossy && bounce == renderer.bounces - 1) {
                                    visible_points[index] = Some(VisiblePoint {
                                        position: surface_point.position,
                                        out_direction: -ray.direction,
                                        bsdf: material,
                                    });

                                    pixel_throughput.copy_from_slice(&throughput);

                                    return None;
                                }

                                throughput[index] *= output.reflectivity.value()
                                    * output.in_direction.dot(shading_normal_vector).abs()
                                    / output.pdf;
                                specular_bounce.set(index, !diffuse);

                                if throughput[index] < 0.25 {
                                    let continue_probability = throughput[index].min(1.0);

                                    if tools.sampler.gen_f32() > continue_probability {
                                        return None;
                                    }

                                    throughput[index] /= continue_probability;
                                }

                                Some((
                                    index,
                                    Ray3::new(surface_point.position, output.in_direction),
                                ))
                            });

                    rays.clear();
                    rays.extend(new_rays);

                    LightMode::Dispersed(&*rays)
                }
            }
        }
    }

    //progress.set_progress(tile.area() as u64 - 1);
}

fn trace_photons(
    batch_index: usize,
    number_of_photons: u64,
    pixels: &[VisiblePixels],
    rng: &mut XorShiftRng,
    world: &World,
    resources: Resources,
    renderer: &Renderer,
    wavelengths: &Wavelengths,
) {
    let mut exe = ExecutionContext::new(resources);
    let arena = Bump::new();
    let light_pool = LightPool::new(&arena, renderer.spectrum_bins);
    let interaction_output_pool = SlicePool::new(&arena, renderer.spectrum_bins);

    let mut tools = Tools {
        sampler: rng,
        light_pool: &light_pool,
        interaction_output_pool: &interaction_output_pool,
        execution_context: &mut exe,
    };

    //let message = format!("Photon batch {}", index + 1);
    //progress.show(&message, number_of_photons);
    //let mut last_progress = Instant::now();

    let mut rays = Vec::with_capacity(renderer.spectrum_bins);
    let mut interactions = Vec::with_capacity(renderer.spectrum_bins);

    for photon in 0..number_of_photons {
        //if Instant::now() - last_progress > Duration::from_millis(1000) {
        //    progress.set_progress(photon);
        //    last_progress = Instant::now();
        //}

        let lamps = &world.lights[..];
        let num_lamps = lamps.len();
        let lamp = tools
            .sampler
            .select(lamps)
            .expect("there has to be at least one light source");
        let lamp_pdf = 1.0 / num_lamps as f32;

        let lamp_sample = lamp.sample_emission_out(wavelengths, &mut tools);
        if lamp_sample.pdf_pos == 0.0 || lamp_sample.pdf_dir == 0.0 || lamp_sample.light.is_black()
        {
            continue;
        }

        let mut throughput: CoherentLight = lamp_sample.light
            * lamp_sample.normal.dot(lamp_sample.ray.direction).abs()
            / (lamp_pdf * lamp_sample.pdf_pos * lamp_sample.pdf_dir);

        if throughput.is_black() {
            continue;
        }

        rays.push((lamp_sample.ray, LightMode::Coherent(())));

        for bounce in 0..renderer.bounces {
            for (ray, light_mode) in rays.drain(..) {
                let intersection = if let Some(intersection) = world.intersect(ray) {
                    intersection
                } else {
                    continue;
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

                if bounce > 0 {
                    let visible_pixels = pixels.iter().flat_map(|pixels| {
                        pixels.point_intersect(intersection.surface_point.position)
                    });
                    for &(pixel, visible_point, pixel_light_mode) in visible_pixels {
                        let distance_squared = visible_point
                            .position
                            .distance2(intersection.surface_point.position);
                        if distance_squared > pixel.radius * pixel.radius {
                            continue;
                        }

                        let increment = match (light_mode, pixel_light_mode) {
                            (LightMode::Coherent(()), LightMode::Coherent(())) => {
                                let phi = visible_point.bsdf.evaluate_coherent(
                                    visible_point.out_direction,
                                    shading_normal,
                                    -ray.direction,
                                    surface_data.texture,
                                    wavelengths,
                                    &mut tools,
                                ) * &throughput;

                                for (bin, &phi) in pixel.phi.iter().zip(phi.iter()) {
                                    bin.add_assign(phi);
                                }

                                phi.len()
                            }
                            (LightMode::Coherent(()), LightMode::Dispersed(index))
                            | (LightMode::Dispersed(index), LightMode::Coherent(())) => {
                                let phi = visible_point.bsdf.evaluate_dispersed(
                                    visible_point.out_direction,
                                    shading_normal,
                                    -ray.direction,
                                    surface_data.texture,
                                    index,
                                    wavelengths,
                                    &mut tools,
                                ) * throughput[index];

                                pixel.phi[index].add_assign(phi.value());

                                1
                            }
                            (LightMode::Dispersed(index), LightMode::Dispersed(pixel_index)) => {
                                if pixel_index != index {
                                    continue;
                                }

                                let phi = visible_point.bsdf.evaluate_dispersed(
                                    visible_point.out_direction,
                                    shading_normal,
                                    -ray.direction,
                                    surface_data.texture,
                                    index,
                                    wavelengths,
                                    &mut tools,
                                ) * throughput[index];

                                pixel.phi[index].add_assign(phi.value());

                                1
                            }
                        };

                        pixel.m.fetch_add(increment);
                    }
                }

                if let LightMode::Dispersed(index) = light_mode {
                    let interaction = material.sample_reflection_dispersed(
                        -ray.direction,
                        surface_data.texture,
                        shading_normal,
                        index,
                        &wavelengths,
                        &mut tools,
                    );

                    if let Some(interaction) = interaction {
                        interactions.push(InteractionData {
                            output: LightMode::Dispersed((index, interaction.output)),
                            shading_normal_vector,
                            shading_normal,
                            material,
                            ray,
                            texture_coordinate: surface_data.texture,
                            surface_point: intersection.surface_point,
                            diffuse: interaction.diffuse,
                            glossy: interaction.glossy,
                        });
                    }
                } else {
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
                        continue; // Emissive only, so terminate
                    };

                    let diffuse = interaction.diffuse;
                    let glossy = interaction.glossy;
                    match interaction.output {
                        InteractionOutput::Coherent(output) => {
                            interactions.push(InteractionData {
                                output: LightMode::Coherent(output),
                                shading_normal_vector,
                                shading_normal,
                                material,
                                texture_coordinate: surface_data.texture,
                                ray,
                                surface_point: intersection.surface_point,
                                diffuse,
                                glossy,
                            });
                        }
                        InteractionOutput::Dispersed(output) => {
                            let new_interactions =
                                output
                                    .iter()
                                    .enumerate()
                                    .map(|(index, &output)| InteractionData {
                                        output: LightMode::Dispersed((index, output)),
                                        shading_normal_vector,
                                        shading_normal,
                                        material,
                                        texture_coordinate: surface_data.texture,
                                        ray,
                                        surface_point: intersection.surface_point,
                                        diffuse,
                                        glossy,
                                    });

                            interactions.extend(new_interactions);
                        }
                    }
                }
            }

            rays.extend(interactions.drain(..).filter_map(|interaction_data| {
                let InteractionData {
                    output,
                    shading_normal_vector,
                    surface_point,
                    ..
                } = interaction_data;

                match output {
                    LightMode::Coherent(output) => {
                        if output.reflectivity.is_black() || output.pdf == 0.0 {
                            return None;
                        }

                        let new_throughput = output.reflectivity
                            * &throughput
                            * output.in_direction.dot(shading_normal_vector).abs()
                            / output.pdf;

                        let rr_probability =
                            (1.0f32 - new_throughput.max() / throughput.max()).max(0.0);

                        if tools.sampler.gen_f32() < rr_probability {
                            return None;
                        }

                        throughput = new_throughput / (1.0 - rr_probability);

                        Some((
                            Ray3::new(surface_point.position, output.in_direction),
                            LightMode::Coherent(()),
                        ))
                    }
                    LightMode::Dispersed((index, output)) => {
                        if output.reflectivity.value() == 0.0 || output.pdf == 0.0 {
                            return None;
                        }

                        let new_throughput = output.reflectivity.value()
                            * throughput[index]
                            * output.in_direction.dot(shading_normal_vector).abs()
                            / output.pdf;

                        let rr_probability = (1.0f32 - new_throughput / throughput[index]).max(0.0);

                        if tools.sampler.gen_f32() < rr_probability {
                            return None;
                        }

                        throughput[index] = new_throughput / (1.0 - rr_probability);

                        Some((
                            Ray3::new(surface_point.position, output.in_direction),
                            LightMode::Dispersed(index),
                        ))
                    }
                }
            }));
        }
    }

    //progress.set_progress(number_of_photons - 1);
}

struct Pixel<'a> {
    radius: f32,
    pixel_position: Point2<f32>,
    accumulated_light: &'a mut [f32],
    throughput: &'a mut [f32], // Keeping it outside visible_point for easier allocation
    phi: &'a [AtomicF32],
    m: AtomicCell<usize>,
    n: f32,
    tau: &'a mut [f32],
    visible_point: LightMode<
        Option<VisiblePoint<'a>>,
        PooledSlice<'a, Option<VisiblePoint<'a>>, utils::Mutex>,
    >,
}

impl<'a> Pixel<'a> {
    fn new(radius: f32, bins: usize, samples: usize, arena: &'a Bump) -> Self {
        Pixel {
            radius,
            pixel_position: Point2::new(0.0, 0.0),
            accumulated_light: arena.alloc_slice_fill_default(bins),
            throughput: arena.alloc_slice_fill_default(samples),
            phi: arena.alloc_slice_fill_with(samples, |_| AtomicF32::new(0.0)),
            m: AtomicCell::new(0),
            n: 0.0,
            tau: arena.alloc_slice_fill_default(bins),
            visible_point: LightMode::Coherent(None),
        }
    }
}

impl<'p, 'a> Bounded for (&'p Pixel<'a>, VisiblePoint<'a>, LightMode<(), usize>) {
    fn aabb(&self) -> collision::Aabb3<f32> {
        let (pixel, visible_point, _) = self;

        let radius = Vector3::new(pixel.radius, pixel.radius, pixel.radius);
        Aabb3::new(
            visible_point.position - radius,
            visible_point.position + radius,
        )
    }
}

#[derive(Clone, Copy)]
struct VisiblePoint<'a> {
    position: Point3<f32>,
    out_direction: Vector3<f32>,
    bsdf: Material<'a>,
}

#[derive(Clone, Copy)]
enum LightMode<C, D> {
    Coherent(C),
    Dispersed(D),
}

impl<C, D, I> LightMode<Option<C>, I>
where
    for<'a> &'a I: IntoIterator<Item = &'a Option<D>>,
{
    fn is_some(&self) -> bool {
        match self {
            LightMode::Coherent(option) => option.is_some(),
            LightMode::Dispersed(options) => options.into_iter().all(Option::is_some),
        }
    }
}

impl<T, I> LightMode<Option<T>, I>
where
    for<'a> &'a I: IntoIterator<Item = &'a Option<T>>,
{
    fn iter(&self) -> impl Iterator<Item = &T> {
        match self {
            LightMode::Coherent(option) => LightMode::Coherent(option.as_ref().into_iter()),
            LightMode::Dispersed(options) => LightMode::Dispersed(options.into_iter().flatten()),
        }
    }
}

impl<T, A, B> Iterator for LightMode<A, B>
where
    A: Iterator<Item = T>,
    B: Iterator<Item = T>,
{
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            LightMode::Coherent(iterator) => iterator.next(),
            LightMode::Dispersed(iterator) => iterator.next(),
        }
    }
}

struct InteractionData<'a, T> {
    output: T,
    shading_normal_vector: Vector3<f32>,
    shading_normal: Normal,
    material: Material<'a>,
    ray: Ray3<f32>,
    texture_coordinate: Point2<f32>,
    surface_point: SurfacePoint<'a>,
    diffuse: bool,
    glossy: bool,
}

enum RenderTask<'r, 'a> {
    TraceTile(usize, &'a Tile, &'r mut [Pixel<'a>], &'r Wavelengths),
    TracePhotons(usize, u64, &'r [VisiblePixels<'r, 'a>], &'r Wavelengths),
    WriteToFilm {
        tile: &'a Tile,
        pixels: &'r mut [Pixel<'a>],
        iterations: u64,
        number_of_photons: u64,
    },
}

struct RenderWorker<'a> {
    visible_points_pool: &'a LockedSlicePool<'a, Option<VisiblePoint<'a>>>,
    rng: XorShiftRng,
    camera: &'a Camera,
    film: &'a Film,
    world: &'a World<'a>,
    resources: Resources<'a>,
    renderer: &'a Renderer,
    config: &'a Config,
}

impl<'r, 'a> Worker<RenderTask<'r, 'a>> for RenderWorker<'a> {
    type Output = ();

    fn do_work(&mut self, task: RenderTask<'r, 'a>) {
        match task {
            RenderTask::TraceTile(tile_index, tile, pixels, wavelengths) => trace_tile(
                tile_index,
                tile,
                pixels,
                self.visible_points_pool,
                &mut self.rng,
                self.camera,
                self.film,
                self.world,
                self.resources,
                self.renderer,
                self.config,
                wavelengths,
            ),
            RenderTask::TracePhotons(index, number_of_photons, pixels, wavelengths) => {
                trace_photons(
                    index,
                    number_of_photons,
                    pixels,
                    &mut self.rng,
                    self.world,
                    self.resources,
                    self.renderer,
                    wavelengths,
                )
            }
            RenderTask::WriteToFilm {
                tile,
                pixels,
                iterations,
                number_of_photons,
            } => {
                for (area_iter, pixel) in (0..tile.area()).zip(pixels) {
                    let samples = pixel.accumulated_light.iter().zip(&*pixel.tau).enumerate();
                    for (index, (accumulated_light, tau)) in samples {
                        let light = accumulated_light / iterations as f32;

                        let tau = tau
                            / (number_of_photons as f32
                                * std::f32::consts::PI
                                * pixel.radius
                                * pixel.radius);

                        self.film
                            .overwrite(pixel.pixel_position, index, light + tau, 1.0);
                    }
                }
            }
        }
    }
}

#[derive(Clone, Copy)]
struct BvhWorker;

impl<'r, 'a> Worker<&'r [Pixel<'a>]> for BvhWorker {
    type Output = VisiblePixels<'r, 'a>;

    fn do_work(&mut self, pixels: &'r [Pixel<'a>]) -> Self::Output {
        let visible_pixels = pixels
            .iter()
            .flat_map(|pixel| match (pixel, &pixel.visible_point) {
                (pixel, &LightMode::Coherent(coherent)) => LightMode::Coherent(
                    coherent
                        .into_iter()
                        .map(move |visible_point| (pixel, visible_point, LightMode::Coherent(()))),
                ),
                (pixel, LightMode::Dispersed(dispersed)) => {
                    LightMode::Dispersed(dispersed.iter().copied().enumerate().filter_map(
                        move |(index, visible_point)| {
                            visible_point.map(|visible_point| {
                                (pixel, visible_point, LightMode::Dispersed(index))
                            })
                        },
                    ))
                }
            })
            .collect();

        Bvh::new(visible_pixels)
    }
}
