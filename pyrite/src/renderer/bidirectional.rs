use rand::{self, Rng, SeedableRng};
use rand_xorshift::XorShiftRng;

use cgmath::{EuclideanSpace, InnerSpace, Point2, Vector3};
use collision::Ray3;

use super::{
    algorithm::{contribute, make_tiles, Tile},
    LocalProgress, Progress, Renderer, TaskRunner,
};
use crate::cameras::Camera;
use crate::film::{Film, Sample};
use crate::lamp::{RaySample, Surface};
use crate::tracer::{trace, Bounce, BounceType};
use crate::utils::pairs;
use crate::{
    materials::ProbabilityInput,
    math::DIST_EPSILON,
    program::{ExecutionContext, Resources},
    world::World,
};
use std::{
    cell::Cell,
    time::{Duration, Instant},
};

pub struct BidirParams {
    pub bounces: u32,
}

pub(crate) fn render<F: FnMut(Progress<'_>)>(
    film: &Film,
    task_runner: TaskRunner,
    mut on_status: F,
    renderer: &Renderer,
    config: &BidirParams,
    world: &World,
    camera: &Camera,
    resources: Resources,
) {
    fn gen_rng() -> XorShiftRng {
        XorShiftRng::from_rng(rand::thread_rng()).expect("could not generate RNG")
    }

    let tiles = make_tiles(film.width(), film.height(), renderer.tile_size, camera);

    let status_message = "Rendering";
    on_status(Progress {
        progress: 0,
        message: &status_message,
    });

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

fn render_tile<R: Rng>(
    index: usize,
    mut rng: R,
    tile: Tile,
    film: &Film,
    camera: &Camera,
    world: &World,
    resources: Resources,
    renderer: &Renderer,
    bidir_params: &BidirParams,
    progress: LocalProgress,
) {
    let mut lamp_path = Vec::with_capacity(bidir_params.bounces as usize + 1);
    let mut camera_path = Vec::with_capacity(renderer.bounces as usize);
    let mut additional_samples = Vec::with_capacity(renderer.spectrum_samples as usize - 1);
    let mut exe = ExecutionContext::new(resources);

    let iterations = tile.area() as u64 * renderer.pixel_samples as u64;
    let message = format!("Tile {}", index);
    let mut last_progress = Instant::now();
    progress.show(&message, iterations);

    for i in 0..iterations {
        if Instant::now() - last_progress > Duration::from_millis(100) {
            progress.set_progress(i);
            last_progress = Instant::now();
        }

        lamp_path.clear();
        camera_path.clear();
        additional_samples.clear();

        let position = tile.sample_point(&mut rng);
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

        let camera_ray = camera.ray_towards(&position, &mut rng);
        let lamp_sample = world
            .pick_lamp(&mut rng)
            .and_then(|(l, p)| l.sample_ray(&mut rng).map(|r| (r, p)));
        if let Some((lamp_sample, probability)) = lamp_sample {
            let RaySample {
                mut ray,
                surface,
                weight,
            } = lamp_sample;

            let (color, material_probability, dispersed, normal, texture) = match surface {
                Surface::Physical {
                    normal,
                    material,
                    texture,
                } => {
                    let component = material.choose_emissive(&mut rng);
                    let input = ProbabilityInput {
                        wavelength,
                        wavelength_used: Cell::new(false),
                        normal,
                        incident: -ray.direction,
                        texture_coordinate: texture,
                    };

                    let probability = component.get_probability(&mut exe, &input);

                    (
                        component.bsdf.color,
                        probability,
                        input.wavelength_used.get(),
                        normal,
                        texture,
                    )
                }
                Surface::Color(color) => (color, 1.0, false, ray.direction, Point2::origin()),
            };
            ray.origin += normal * DIST_EPSILON;

            lamp_path.push(Bounce {
                ty: BounceType::Emission,
                dispersed,
                color,
                incident: Vector3::new(0.0, 0.0, 0.0),
                position: ray.origin,
                normal,
                texture,
                probability: weight / (probability * material_probability),
                direct_light: vec![],
            });

            trace(
                &mut lamp_path,
                &mut rng,
                ray,
                wavelength,
                world,
                bidir_params.bounces,
                0,
                &mut exe,
            );

            pairs(&mut lamp_path, |to, from| {
                to.incident = -from.incident;
                if let BounceType::Diffuse(_, ref mut o) = from.ty {
                    *o = from.incident
                }
            });

            if lamp_path.len() > 1 {
                if let Some(last) = lamp_path.pop() {
                    match last.ty {
                        BounceType::Diffuse(_, _) | BounceType::Specular => lamp_path.push(last),
                        BounceType::Emission => {}
                    }
                }
            }
            lamp_path.reverse();
        }

        trace(
            &mut camera_path,
            &mut rng,
            camera_ray,
            wavelength,
            world,
            renderer.bounces,
            renderer.light_samples,
            &mut exe,
        );

        let total = (camera_path.len() * lamp_path.len()) as f32;
        let weight = 1.0 / total;

        let mut use_additional = true;

        for bounce in &camera_path {
            use_additional = !bounce.dispersed && use_additional;
            let additional_samples_slice = if use_additional {
                &mut *additional_samples
            } else {
                &mut []
            };

            contribute(bounce, &mut main_sample, additional_samples_slice, &mut exe);

            for mut contribution in connect_paths(
                &bounce,
                &main_sample,
                &additional_samples,
                &lamp_path,
                world,
                use_additional,
                &mut exe,
            ) {
                contribution.weight = weight;
                film.expose(position, contribution);
            }
        }

        film.expose(position, main_sample.0.clone());

        if use_additional {
            for &(ref sample, _) in &additional_samples {
                film.expose(position, sample.clone());
            }
        }

        let weight = 1.0 / lamp_path.len() as f32;
        for (i, bounce) in lamp_path.iter().enumerate() {
            if let BounceType::Diffuse(_, _) = bounce.ty {
            } else {
                continue;
            }

            let camera_hit = camera.is_visible(bounce.position, &world, &mut rng);
            if let Some((position, ray)) = camera_hit {
                if position.x > -1.0 && position.x < 1.0 && position.y > -1.0 && position.y < 1.0 {
                    let sq_distance = (ray.origin - bounce.position).magnitude2();
                    let scale = 1.0 / (sq_distance);
                    let brdf_in = bounce.ty.brdf(-ray.direction, bounce.normal)
                        / bounce.ty.brdf(bounce.incident, bounce.normal);

                    main_sample.0.brightness = 0.0;
                    main_sample.0.weight = weight;
                    main_sample.1 = scale;

                    use_additional = true;
                    for &mut (ref mut sample, ref mut reflectance) in &mut additional_samples {
                        sample.brightness = 0.0;
                        sample.weight = weight;
                        *reflectance = scale;
                    }

                    for (i, bounce) in lamp_path[i..].iter().enumerate() {
                        use_additional = !bounce.dispersed && use_additional;
                        let additional_samples = if use_additional {
                            &mut *additional_samples
                        } else {
                            &mut []
                        };

                        contribute(bounce, &mut main_sample, additional_samples, &mut exe);

                        if i == 0 {
                            main_sample.1 *= brdf_in;
                            for (_, reflectance) in additional_samples {
                                *reflectance *= brdf_in;
                            }
                        }
                    }

                    film.expose(position, main_sample.0.clone());

                    if use_additional {
                        for &(ref sample, _) in &additional_samples {
                            film.expose(position, sample.clone());
                        }
                    }
                }
            }
        }
    }
}

fn connect_paths<'a>(
    bounce: &Bounce<'a>,
    main: &(Sample, f32),
    additional: &[(Sample, f32)],
    path: &[Bounce<'a>],
    world: &World,
    use_additional: bool,
    exe: &mut ExecutionContext<'a>,
) -> Vec<Sample> {
    let mut contributions = vec![];
    let bounce_brdf = match bounce.ty {
        BounceType::Emission | BounceType::Specular => return contributions,
        BounceType::Diffuse(brdf, _) => brdf,
    };

    for (i, lamp_bounce) in path.iter().enumerate() {
        if let BounceType::Specular = lamp_bounce.ty {
            continue;
        }

        let from = bounce.position;
        let to = lamp_bounce.position;

        let direction = to - from;
        let sq_distance = direction.magnitude2();
        let distance = sq_distance.sqrt();
        let ray = Ray3::new(from, direction / distance);

        if bounce.normal.dot(ray.direction) <= 0.0 {
            continue;
        }

        if lamp_bounce.normal.dot(-ray.direction) <= 0.0 {
            continue;
        }

        let hit = world.intersect(ray).map(|hit| hit.distance);
        if let Some(dist) = hit {
            if dist < distance - DIST_EPSILON {
                continue;
            }
        }

        let cos_out = bounce.normal.dot(ray.direction).abs();
        let cos_in = lamp_bounce.normal.dot(-ray.direction).abs();
        let brdf_out = bounce_brdf(bounce.incident, bounce.normal, ray.direction)
            / bounce.ty.brdf(bounce.incident, bounce.normal);

        let scale = cos_in * cos_out * brdf_out / (2.0 * std::f32::consts::PI * sq_distance);
        let brdf_in = lamp_bounce.ty.brdf(-ray.direction, lamp_bounce.normal)
            / lamp_bounce
                .ty
                .brdf(lamp_bounce.incident, lamp_bounce.normal);

        let mut use_additional = use_additional;
        let mut additional: Vec<_> = additional
            .iter()
            .cloned()
            .map(|(s, r)| (s, r * scale))
            .collect();
        let mut main = main.clone();
        main.1 *= scale;

        for (i, bounce) in path[i..].iter().enumerate() {
            use_additional = !bounce.dispersed && use_additional;
            let additional_samples = if use_additional {
                &mut *additional
            } else {
                &mut []
            };

            contribute(bounce, &mut main, additional_samples, exe);

            if i == 0 {
                main.1 *= brdf_in;
                for (_, reflectance) in additional_samples {
                    *reflectance *= brdf_in;
                }
            }
        }

        contributions.push(main.0);
        if use_additional {
            contributions.extend(additional.into_iter().map(|(s, _)| s));
        }
    }

    contributions
}
