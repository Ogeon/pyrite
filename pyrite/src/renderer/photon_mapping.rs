use std::sync::{Arc, RwLock};

use rand::{self, Rng, SeedableRng};
use rand_xorshift::XorShiftRng;

use cgmath::{InnerSpace, Point2, Point3, Vector3};

use crate::cameras::Camera;
use crate::film::{Film, Pixel, Sample};
use crate::lamp::Surface;
use crate::renderer::algorithm::contribute;
use crate::renderer::{Renderer, Status, WorkPool};
use crate::spatial::kd_tree::{self, KdTree};
use crate::spatial::Dim3;
use crate::tracer::{trace, Bounce, BounceType, Light, RenderContext};
use crate::utils::{pairs, BatchRange};
use crate::world::World;

pub fn render<W: WorkPool, F: FnMut(Status<'_>)>(
    film: &Film,
    workers: &mut W,
    mut on_status: F,
    renderer: &Renderer,
    config: &Config,
    world: &World<XorShiftRng>,
    camera: &Camera,
) {
    fn gen_rng() -> XorShiftRng {
        XorShiftRng::from_rng(rand::thread_rng()).expect("could not generate RNG")
    }

    let num_tiles = film.num_tiles();
    let mut progress;

    let num_passes = renderer.pixel_samples as usize * config.photon_passes;

    let photon_probability = 1.0
        / (renderer.bounces as f64 * config.photon_bounces as f64 * config.photon_passes as f64);

    for pixel_pass in 0..renderer.pixel_samples {
        let status_message = format!(
            "(pass {}/{}): observing the scene",
            pixel_pass as usize * config.photon_passes,
            num_passes
        );
        let mut camera_bounces = Vec::with_capacity(num_tiles);
        on_status(Status {
            progress: 0,
            message: &status_message,
        });
        progress = 0;
        workers.do_work(
            film.into_iter().map(|f| (f, gen_rng())),
            |(mut tile, mut rng)| {
                let mut all_bounces = vec![];
                for _ in 0..tile.area() as usize {
                    let position = tile.sample_point(&mut rng);
                    let ray = camera.ray_towards(&position, &mut rng);
                    let wavelength = film.sample_wavelength(&mut rng);
                    let light = Light::new(wavelength);

                    let bounces = trace(
                        &mut rng,
                        ray,
                        light,
                        world,
                        renderer.bounces,
                        renderer.light_samples,
                    );
                    let p = 1.0 / renderer.bounces as f64;

                    let mut sample = Sample {
                        wavelength: wavelength,
                        brightness: 0.0,
                        weight: 1.0,
                    };
                    let mut reflectance = 1.0;

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

                    let mut current = Parent::Source(position);
                    for bounce in bounces {
                        for &mut (ref mut sample, ref mut reflectance) in &mut additional_samples {
                            used_additional =
                                contribute(&bounce, sample, reflectance, true) && used_additional;
                        }
                        contribute(&bounce, &mut sample, &mut reflectance, false);

                        match bounce.ty {
                            BounceType::Diffuse(_, _) => {
                                let b = Arc::new(CameraBounce {
                                    parent: current,
                                    bounce: bounce,
                                    pixel: RwLock::new(film.new_pixel()),
                                    position: position,
                                    probability: p,
                                });
                                current = Parent::Bounce(b.clone());
                                all_bounces.push(b);
                            }
                            BounceType::Specular => {
                                let b = Arc::new(CameraBounce {
                                    parent: current,
                                    bounce: bounce,
                                    pixel: RwLock::new(film.new_pixel()),
                                    position: position,
                                    probability: p,
                                });
                                current = Parent::Bounce(b.clone());
                            }
                            BounceType::Emission => break,
                        }
                    }

                    tile.expose(position, sample);
                    if used_additional {
                        for (sample, _) in additional_samples {
                            tile.expose(position, sample);
                        }
                    }
                }
                all_bounces
            },
            |_i, bounces| {
                camera_bounces.extend(bounces);
                progress += 1;
                on_status(Status {
                    progress: ((progress * 100) / num_tiles) as u8,
                    message: &status_message,
                });
            },
        );

        for photon_pass in 0..config.photon_passes {
            let mut light_bounces = Vec::with_capacity(config.photons);
            let status_message = format!(
                "(pass {}/{}): shooting photons",
                photon_pass + pixel_pass as usize * config.photon_passes,
                num_passes
            );
            on_status(Status {
                progress: 0,
                message: &status_message,
            });
            progress = 0;
            workers.do_work(
                BatchRange::new(0..config.photons, 5000).map(|batch| {
                    let rng: XorShiftRng = gen_rng();
                    (batch, rng)
                }),
                |(num_rays, mut rng)| {
                    let mut processed = vec![];

                    for _ in 0..num_rays {
                        let res = world
                            .pick_lamp(&mut rng)
                            .and_then(|(lamp, p)| lamp.sample_ray(&mut rng).map(|s| (lamp, p, s)));

                        if let Some((_lamp, probability, mut ray_sample)) = res {
                            let mut light = Light::new(film.sample_wavelength(&mut rng));

                            let (color, normal) = match ray_sample.surface {
                                Surface::Physical { normal, material } => {
                                    let color = material.get_emission(
                                        &mut light,
                                        -ray_sample.ray.direction,
                                        normal,
                                        &mut rng,
                                    );
                                    (color, normal)
                                }
                                Surface::Color(color) => (Some(color), ray_sample.ray),
                            };

                            if let Some(color) = color {
                                ray_sample.ray.origin += normal.direction * 0.00001;

                                let mut bounces = trace(
                                    &mut rng,
                                    ray_sample.ray,
                                    light.clone(),
                                    world,
                                    config.photon_bounces,
                                    0,
                                );
                                let p = 1.0 / config.photon_bounces as f64;

                                let incident = bounces
                                    .get(0)
                                    .map(|b| -b.incident)
                                    .unwrap_or(Vector3::new(0.0, 0.0, 0.0));

                                let mut current = Arc::new(LightBounce {
                                    parent: None,
                                    bounce: Bounce {
                                        ty: BounceType::Emission,
                                        light: light,
                                        color: color,
                                        incident: incident,
                                        normal: normal,
                                        probability: ray_sample.weight * probability,
                                        direct_light: vec![],
                                    },
                                    probability: p,
                                });

                                if let Some(bounce) = bounces.get_mut(0) {
                                    if let BounceType::Diffuse(_, ref mut o) = bounce.ty {
                                        *o = -incident
                                    }
                                }

                                pairs(&mut bounces, |to, from| {
                                    to.incident = -from.incident;
                                    if let BounceType::Diffuse(_, ref mut o) = from.ty {
                                        *o = from.incident
                                    }
                                });

                                for bounce in bounces {
                                    match bounce.ty {
                                        BounceType::Diffuse(_, _) => {
                                            let b = Arc::new(LightBounce {
                                                parent: Some(current),
                                                bounce: bounce,
                                                probability: p,
                                            });
                                            current = b.clone();
                                            processed.push(b);
                                        }
                                        BounceType::Specular => {
                                            let b = Arc::new(LightBounce {
                                                parent: Some(current),
                                                bounce: bounce,
                                                probability: p,
                                            });
                                            current = b.clone();
                                        }
                                        BounceType::Emission => break,
                                    }
                                }
                            }
                        }
                    }

                    (num_rays, processed)
                },
                |_, (n, bounces)| {
                    light_bounces.extend(bounces);
                    progress += n;
                    on_status(Status {
                        progress: ((progress as f64 / config.photons as f64) * 100.0) as u8,
                        message: &status_message,
                    });
                },
            );

            let light_bounces = KdTree::new(light_bounces, 100);

            let status_message = format!(
                "(pass {}/{}): gathering light",
                photon_pass + pixel_pass as usize * config.photon_passes,
                num_passes
            );
            on_status(Status {
                progress: 0,
                message: &status_message,
            });
            progress = 0;

            workers.do_work(
                camera_bounces.chunks(5000).map(|b| (b, gen_rng())),
                |(bounces, mut rng)| {
                    for hit in bounces {
                        let mut pixel = hit.pixel.write().expect("failed to write to sample point");
                        let point = KdPoint(hit.bounce.normal.origin);
                        let neighbors: Vec<_> =
                            light_bounces.neighbors(&point, config.radius).collect();
                        let num_neighbors = neighbors.len();
                        for neighbor in neighbors {
                            let mut bounce_light = hit.bounce.light.clone();
                            let mut neighbor_light = neighbor.bounce.light.clone();

                            if bounce_light.is_white() || neighbor_light.is_white() {
                                let (use_additional, wavelength) =
                                    if bounce_light.is_white() && neighbor_light.is_white() {
                                        (true, neighbor_light.colored())
                                    } else if bounce_light.is_white() {
                                        (false, neighbor_light.colored())
                                    } else {
                                        (false, bounce_light.colored())
                                    };

                                let mut samples = vec![(
                                    Sample {
                                        wavelength: wavelength,
                                        brightness: 0.0,
                                        weight: photon_probability / num_neighbors as f64,
                                    },
                                    1.0,
                                )];
                                if use_additional {
                                    samples.extend((0..renderer.spectrum_samples).map(|_| {
                                        (
                                            Sample {
                                                wavelength: film.sample_wavelength(&mut rng),
                                                brightness: 0.0,
                                                weight: photon_probability / num_neighbors as f64,
                                            },
                                            1.0,
                                        )
                                    }));
                                }

                                let incident = -neighbor.bounce.incident;

                                let mut weight = incident.dot(hit.bounce.normal.direction).max(0.0);
                                if weight > 0.0 {
                                    weight *= hit
                                        .bounce
                                        .incident
                                        .dot(-hit.bounce.normal.direction)
                                        .max(0.0);
                                    weight /= ::std::f64::consts::PI;
                                    hit.accumulate_reflectance(&mut samples, incident);
                                    neighbor.accumulate_light(&mut samples);
                                }

                                for (mut sample, _) in samples {
                                    sample.brightness *= weight; // (::std::f64::consts::PI * config.radius * config.radius);
                                    pixel.add_sample(film.to_pixel_sample(&sample));
                                }
                            }
                        }

                        if num_neighbors == 0 {
                            for _ in 0..renderer.spectrum_samples + 1 {
                                let sample = Sample {
                                    wavelength: film.sample_wavelength(&mut rng),
                                    brightness: 0.0,
                                    weight: 1.0
                                        / (renderer.bounces as f64 * config.photon_passes as f64),
                                };

                                pixel.add_sample(film.to_pixel_sample(&sample));
                            }
                        }
                    }

                    bounces.len()
                },
                |_, n| {
                    progress += n;
                    on_status(Status {
                        progress: ((progress as f64 / camera_bounces.len() as f64) * 100.0) as u8,
                        message: &status_message,
                    });
                },
            );
        }

        film.merge_pixels(camera_bounces.iter().map(|b| {
            (
                b.position,
                b.pixel.read().expect("could not read pixel").clone(),
            )
        }));
    }
}

pub struct Config {
    pub radius: f64,
    pub photons: usize,
    pub photon_bounces: u32,
    pub photon_passes: usize,
}

struct CameraBounce<'a> {
    parent: Parent<CameraBounce<'a>, Point2<f64>>,
    bounce: Bounce<'a>,
    pixel: RwLock<Pixel>,
    position: Point2<f64>,
    probability: f64,
}

impl<'a> CameraBounce<'a> {
    fn accumulate_reflectance(&self, samples: &mut [(Sample, f64)], exit: Vector3<f64>) {
        let mut current = Some(self);
        let mut first_brdf = if let BounceType::Diffuse(brdf, _) = self.bounce.ty {
            Some((brdf, exit))
        } else {
            None
        };

        while let Some(hit) = current {
            let &Bounce {
                ref ty,
                light: _,
                color,
                incident,
                normal,
                probability,
                ..
            } = &hit.bounce;

            let brdf = if let Some((brdf, ray_out)) = first_brdf.take() {
                brdf(incident, normal.direction, ray_out)
            } else {
                ty.brdf(incident, normal.direction)
            };

            for &mut (ref sample, ref mut reflectance) in &mut *samples {
                let context = RenderContext {
                    wavelength: sample.wavelength,
                    incident: incident,
                    normal: normal.direction,
                };
                let c = color.get(&context);
                *reflectance *= c * probability * brdf;
            }

            match hit.parent {
                Parent::Bounce(ref b) => current = Some(b),
                Parent::Source(_) => current = None,
            }
        }
    }
}

enum Parent<B, S> {
    Bounce(Arc<B>),
    Source(S),
}

struct KdPoint(Point3<f64>);

impl kd_tree::Point for KdPoint {
    type Dim = Dim3;

    fn get(&self, axis: Dim3) -> f64 {
        match axis {
            Dim3::X => self.0.x,
            Dim3::Y => self.0.y,
            Dim3::Z => self.0.z,
        }
    }
}

struct LightBounce<'a> {
    parent: Option<Arc<LightBounce<'a>>>,
    bounce: Bounce<'a>,
    probability: f64,
}

impl<'a> LightBounce<'a> {
    fn accumulate_light(&self, samples: &mut [(Sample, f64)]) {
        let mut current = self.parent.as_ref().map(|p| &**p);

        for &mut (ref _sample, ref mut reflectance) in &mut *samples {
            *reflectance *= self.bounce.probability
        }

        while let Some(hit) = current {
            let &Bounce {
                ref ty,
                light: _,
                color,
                incident,
                normal,
                probability,
                ..
            } = &hit.bounce;

            for &mut (ref mut sample, ref mut reflectance) in &mut *samples {
                let context = RenderContext {
                    wavelength: sample.wavelength,
                    incident: incident,
                    normal: normal.direction,
                };

                let c = color.get(&context) * probability;

                if let BounceType::Emission = *ty {
                    sample.brightness = c * *reflectance;
                } else {
                    *reflectance *= c * ty.brdf(incident, normal.direction);
                }
            }

            current = hit.parent.as_ref().map(|p| &**p);
        }
    }
}

struct LightSource<'a, R: Rng> {
    surface: Surface<'a, R>,
    weight: f64,
}

impl<'a> kd_tree::Element for Arc<LightBounce<'a>> {
    type Point = KdPoint;

    fn position(&self) -> KdPoint {
        KdPoint(self.bounce.normal.origin)
    }

    fn sq_distance(&self, point: &KdPoint) -> f64 {
        (self.bounce.normal.origin - point.0).magnitude2()
    }
}
