use rand::{self, Rng, SeedableRng};
use rand_xorshift::XorShiftRng;

use cgmath::{InnerSpace, Vector3};
use collision::Ray3;

use super::algorithm::{make_tiles, Tile};
use crate::cameras::Camera;
use crate::film::{Film, Sample};
use crate::lamp::{RaySample, Surface};
use crate::renderer::algorithm::contribute;
use crate::renderer::{Renderer, Status, WorkPool};
use crate::tracer::{trace, Bounce, BounceType, Light};
use crate::utils::pairs;
use crate::world::World;

pub struct BidirParams {
    pub bounces: u32,
}

pub fn render<W: WorkPool, F: FnMut(Status<'_>)>(
    film: &Film,
    workers: &mut W,
    mut on_status: F,
    renderer: &Renderer,
    config: &BidirParams,
    world: &World<XorShiftRng>,
    camera: &Camera,
) {
    fn gen_rng() -> XorShiftRng {
        XorShiftRng::from_rng(rand::thread_rng()).expect("could not generate RNG")
    }

    let tiles = make_tiles(film.width(), film.height(), renderer.tile_size, camera);

    let status_message = "rendering";
    on_status(Status {
        progress: 0,
        message: &status_message,
    });

    let mut progress: usize = 0;
    let num_tiles = tiles.len();

    workers.do_work(
        tiles.into_iter().map(|f| (f, gen_rng())),
        |(tile, rng)| {
            render_tile(rng, tile, film, camera, world, renderer, config);
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
    tile: Tile,
    film: &Film,
    camera: &Camera,
    world: &World<R>,
    renderer: &Renderer,
    bidir_params: &BidirParams,
) {
    let mut lamp_path = Vec::with_capacity(bidir_params.bounces as usize + 1);

    for _ in 0..(tile.area() * renderer.pixel_samples as usize) {
        lamp_path.clear();

        let position = tile.sample_point(&mut rng);
        let wavelength = film.sample_wavelength(&mut rng);
        let light = Light::new(wavelength);

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

            let mut light = light.clone();
            let (color, normal) = match surface {
                Surface::Physical { normal, material } => {
                    let color = material.get_emission(&mut light, -ray.direction, normal, &mut rng);
                    (color, normal)
                }
                Surface::Color(color) => (Some(color), ray),
            };
            ray.origin += normal.direction * 0.00001;

            if let Some(color) = color {
                lamp_path.push(Bounce {
                    ty: BounceType::Emission,
                    light: light.clone(),
                    color: color,
                    incident: Vector3::new(0.0, 0.0, 0.0),
                    normal: normal,
                    probability: weight / probability,
                    direct_light: vec![],
                });

                lamp_path.extend(trace(&mut rng, ray, light, world, bidir_params.bounces, 0));

                pairs(&mut lamp_path, |to, from| {
                    to.incident = -from.incident;
                    if let BounceType::Diffuse(_, ref mut o) = from.ty {
                        *o = from.incident
                    }
                });

                if lamp_path.len() > 1 {
                    if let Some(last) = lamp_path.pop() {
                        match last.ty {
                            BounceType::Diffuse(_, _) | BounceType::Specular => {
                                lamp_path.push(last)
                            }
                            BounceType::Emission => {}
                        }
                    }
                }
                lamp_path.reverse();
            }
        }

        let camera_path = trace(
            &mut rng,
            camera_ray,
            light,
            world,
            renderer.bounces,
            renderer.light_samples,
        );

        let total = (camera_path.len() * lamp_path.len()) as f64;
        let weight = 1.0 / total;

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
                        wavelength: film.sample_wavelength(&mut rng),
                        brightness: 0.0,
                        weight: 1.0,
                    },
                    1.0,
                )
            })
            .collect();

        for bounce in camera_path {
            for &mut (ref mut sample, ref mut reflectance) in &mut additional_samples {
                used_additional = contribute(&bounce, sample, reflectance, true) && used_additional;
            }

            {
                let (ref mut sample, ref mut reflectance) = main_sample;
                contribute(&bounce, sample, reflectance, false);
            }

            for mut contribution in connect_paths(
                &bounce,
                &main_sample,
                &additional_samples,
                &lamp_path,
                world,
                used_additional,
            ) {
                contribution.weight = weight;
                film.expose(position, contribution);
            }
        }

        film.expose(position, main_sample.0.clone());

        if used_additional {
            for &(ref sample, _) in &additional_samples {
                film.expose(position, sample.clone());
            }
        }

        let weight = 1.0 / lamp_path.len() as f64;
        for (i, bounce) in lamp_path.iter().enumerate() {
            if let BounceType::Diffuse(_, _) = bounce.ty {
            } else {
                continue;
            }

            let camera_hit = camera.is_visible(bounce.normal.origin, &world, &mut rng);
            if let Some((position, ray)) = camera_hit {
                if position.x > -1.0 && position.x < 1.0 && position.y > -1.0 && position.y < 1.0 {
                    let sq_distance = (ray.origin - bounce.normal.origin).magnitude2();
                    let scale = 1.0 / (sq_distance);
                    let brdf_in = bounce.ty.brdf(-ray.direction, bounce.normal.direction)
                        / bounce.ty.brdf(bounce.incident, bounce.normal.direction);

                    main_sample.0.brightness = 0.0;
                    main_sample.0.weight = weight;
                    main_sample.1 = scale;

                    used_additional = true;
                    for &mut (ref mut sample, ref mut reflectance) in &mut additional_samples {
                        sample.brightness = 0.0;
                        sample.weight = weight;
                        *reflectance = scale;
                    }

                    for (i, bounce) in lamp_path[i..].iter().enumerate() {
                        for &mut (ref mut sample, ref mut reflectance) in &mut additional_samples {
                            used_additional =
                                contribute(bounce, sample, reflectance, true) && used_additional;
                            if i == 0 {
                                *reflectance *= brdf_in;
                            }
                        }

                        let (ref mut sample, ref mut reflectance) = main_sample;
                        contribute(bounce, sample, reflectance, false);
                        if i == 0 {
                            *reflectance *= brdf_in;
                        }
                    }

                    film.expose(position, main_sample.0.clone());

                    if used_additional {
                        for &(ref sample, _) in &additional_samples {
                            film.expose(position, sample.clone());
                        }
                    }
                }
            }
        }
    }
}

fn connect_paths<R: Rng>(
    bounce: &Bounce<'_>,
    main: &(Sample, f64),
    additional: &[(Sample, f64)],
    path: &[Bounce<'_>],
    world: &World<R>,
    use_additional: bool,
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

        let from = bounce.normal.origin;
        let to = lamp_bounce.normal.origin;

        let direction = to - from;
        let ray = Ray3::new(from, direction.normalize());
        let sq_distance = direction.magnitude2();

        if bounce.normal.direction.dot(ray.direction) <= 0.0 {
            continue;
        }

        if lamp_bounce.normal.direction.dot(-ray.direction) <= 0.0 {
            continue;
        }

        let hit = world
            .intersect(&ray)
            .map(|(hit_normal, _)| (hit_normal.origin - from).magnitude2());
        if let Some(dist) = hit {
            if dist < sq_distance - 0.0000001 {
                continue;
            }
        }

        let cos_out = bounce.normal.direction.dot(ray.direction).abs();
        let cos_in = lamp_bounce.normal.direction.dot(-ray.direction).abs();
        let brdf_out = bounce_brdf(bounce.incident, bounce.normal.direction, ray.direction)
            / bounce.ty.brdf(bounce.incident, bounce.normal.direction);

        let scale = cos_in * cos_out * brdf_out / (2.0 * std::f64::consts::PI * sq_distance);
        let brdf_in = lamp_bounce
            .ty
            .brdf(-ray.direction, lamp_bounce.normal.direction)
            / lamp_bounce
                .ty
                .brdf(lamp_bounce.incident, lamp_bounce.normal.direction);

        let mut use_additional = use_additional;
        let mut additional: Vec<_> = additional
            .iter()
            .cloned()
            .map(|(s, r)| (s, r * scale))
            .collect();
        let mut main = main.clone();
        main.1 *= scale;

        for (i, bounce) in path[i..].iter().enumerate() {
            for &mut (ref mut sample, ref mut reflectance) in &mut additional {
                use_additional = contribute(bounce, sample, reflectance, true) && use_additional;
                if i == 0 {
                    *reflectance *= brdf_in;
                }
            }

            let (ref mut sample, ref mut reflectance) = main;
            contribute(bounce, sample, reflectance, false);
            if i == 0 {
                *reflectance *= brdf_in;
            }
        }

        contributions.push(main.0);
        if use_additional {
            contributions.extend(additional.into_iter().map(|(s, _)| s));
        }
    }

    contributions
}
