use std::sync::Arc;

use rand::{self, SeedableRng};
use rand_xorshift::XorShiftRng;

use cgmath::{EuclideanSpace, InnerSpace, Point2, Point3, Vector3};

use super::algorithm::make_tiles;
use crate::cameras::Camera;
use crate::film::{DetachedPixel, Film, Sample};
use crate::lamp::Surface;
use crate::renderer::algorithm::contribute;
use crate::renderer::{Renderer, Status, WorkPool};
use crate::spatial::kd_tree::{self, KdTree};
use crate::spatial::Dim3;
use crate::tracer::{trace, Bounce, BounceType, Light, RenderContext};
use crate::utils::{pairs, BatchRange};
use crate::{
    math::DIST_EPSILON,
    project::program::{ExecutionContext, Resources},
    world::World,
};

pub(crate) fn render<W: WorkPool, F: FnMut(Status<'_>)>(
    film: &Film,
    workers: &mut W,
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

    let tiles = make_tiles(film.width(), film.height(), renderer.tile_size, camera);

    let num_tiles = tiles.len();
    let mut progress;

    let num_passes = renderer.pixel_samples as usize * config.photon_passes;

    let photon_probability = 1.0
        / (renderer.bounces as f32 * config.photon_bounces as f32 * config.photon_passes as f32);

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
            tiles.iter().map(|f| (f, gen_rng())),
            |(tile, mut rng)| {
                let mut all_bounces = vec![];
                let mut bounces = Vec::with_capacity(renderer.bounces as usize);
                let mut exe = ExecutionContext::new(resources);

                for _ in 0..tile.area() as usize {
                    bounces.clear();

                    let position = tile.sample_point(&mut rng);
                    let ray = camera.ray_towards(&position, &mut rng);
                    let wavelength = film.sample_wavelength(&mut rng);
                    let light = Light::new(wavelength);

                    trace(
                        &mut bounces,
                        &mut rng,
                        ray,
                        light,
                        world,
                        renderer.bounces,
                        renderer.light_samples,
                        &mut exe,
                    );
                    let p = 1.0 / renderer.bounces as f32;

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
                                    wavelength: film.sample_wavelength(&mut rng),
                                    brightness: 0.0,
                                    weight: 1.0,
                                },
                                1.0,
                            )
                        })
                        .collect();

                    let mut current = Parent::Source(position);
                    for bounce in bounces.drain(..) {
                        for &mut (ref mut sample, ref mut reflectance) in &mut additional_samples {
                            used_additional =
                                contribute(&bounce, sample, reflectance, true, &mut exe)
                                    && used_additional;
                        }
                        contribute(&bounce, &mut sample, &mut reflectance, false, &mut exe);

                        match bounce.ty {
                            BounceType::Diffuse(_, _) => {
                                let b = Arc::new(CameraBounce {
                                    parent: current,
                                    bounce: bounce,
                                    pixel: film
                                        .get_pixel_ref_f(position)
                                        .expect("position out of bounds"),
                                    probability: p,
                                });
                                current = Parent::Bounce(b.clone());
                                all_bounces.push(b);
                            }
                            BounceType::Specular => {
                                let b = Arc::new(CameraBounce {
                                    parent: current,
                                    bounce: bounce,
                                    pixel: film
                                        .get_pixel_ref_f(position)
                                        .expect("position out of bounds"),
                                    probability: p,
                                });
                                current = Parent::Bounce(b.clone());
                            }
                            BounceType::Emission => break,
                        }
                    }

                    film.expose(position, sample);
                    if used_additional {
                        for (sample, _) in additional_samples {
                            film.expose(position, sample);
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
                    let mut bounces = Vec::with_capacity(renderer.bounces as usize);
                    let mut exe = ExecutionContext::new(resources);

                    for _ in 0..num_rays {
                        bounces.clear();

                        let res = world
                            .pick_lamp(&mut rng)
                            .and_then(|(lamp, p)| lamp.sample_ray(&mut rng).map(|s| (lamp, p, s)));

                        if let Some((_lamp, probability, mut ray_sample)) = res {
                            let mut light = Light::new(film.sample_wavelength(&mut rng));

                            let (color, normal, texture) = match ray_sample.surface {
                                Surface::Physical {
                                    normal,
                                    material,
                                    texture,
                                } => {
                                    let color = material.get_emission(
                                        &mut light,
                                        -ray_sample.ray.direction,
                                        normal,
                                        &mut rng,
                                    );
                                    (color, normal, texture)
                                }
                                Surface::Color(color) => {
                                    (Some(color), ray_sample.ray, Point2::origin())
                                }
                            };

                            if let Some(color) = color {
                                ray_sample.ray.origin += normal.direction * DIST_EPSILON;

                                trace(
                                    &mut bounces,
                                    &mut rng,
                                    ray_sample.ray,
                                    light.clone(),
                                    world,
                                    config.photon_bounces,
                                    0,
                                    &mut exe,
                                );
                                let p = 1.0 / config.photon_bounces as f32;

                                let incident = bounces
                                    .get(0)
                                    .map(|b| -b.incident)
                                    .unwrap_or(Vector3::new(0.0, 0.0, 0.0));

                                let mut current = Arc::new(LightBounce {
                                    parent: None,
                                    bounce: Bounce {
                                        ty: BounceType::Emission,
                                        light,
                                        color,
                                        incident,
                                        normal,
                                        texture,
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

                                for bounce in bounces.drain(..) {
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
                        progress: ((progress as f32 / config.photons as f32) * 100.0) as u8,
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
                    let mut exe = ExecutionContext::new(resources);

                    for hit in bounces {
                        let pixel = &hit.pixel;
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
                                        weight: photon_probability / num_neighbors as f32,
                                    },
                                    1.0,
                                )];
                                if use_additional {
                                    samples.extend((0..renderer.spectrum_samples).map(|_| {
                                        (
                                            Sample {
                                                wavelength: film.sample_wavelength(&mut rng),
                                                brightness: 0.0,
                                                weight: photon_probability / num_neighbors as f32,
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
                                    weight /= ::std::f32::consts::PI;
                                    hit.accumulate_reflectance(&mut samples, incident, &mut exe);
                                    neighbor.accumulate_light(&mut samples, &mut exe);
                                }

                                for (mut sample, _) in samples {
                                    sample.brightness *= weight; // (::std::f32::consts::PI * config.radius * config.radius);
                                    pixel.expose(film.to_pixel_sample(&sample));
                                }
                            }
                        }

                        if num_neighbors == 0 {
                            for _ in 0..renderer.spectrum_samples + 1 {
                                let sample = Sample {
                                    wavelength: film.sample_wavelength(&mut rng),
                                    brightness: 0.0,
                                    weight: 1.0
                                        / (renderer.bounces as f32 * config.photon_passes as f32),
                                };

                                pixel.expose(film.to_pixel_sample(&sample));
                            }
                        }
                    }

                    bounces.len()
                },
                |_, n| {
                    progress += n;
                    on_status(Status {
                        progress: ((progress as f32 / camera_bounces.len() as f32) * 100.0) as u8,
                        message: &status_message,
                    });
                },
            );
        }
    }
}

pub struct Config {
    pub radius: f32,
    pub photons: usize,
    pub photon_bounces: u32,
    pub photon_passes: usize,
}

struct CameraBounce<'a> {
    parent: Parent<CameraBounce<'a>, Point2<f32>>,
    bounce: Bounce<'a>,
    pixel: DetachedPixel<'a>,
    probability: f32,
}

impl<'a> CameraBounce<'a> {
    fn accumulate_reflectance(
        &self,
        samples: &mut [(Sample, f32)],
        exit: Vector3<f32>,
        exe: &mut ExecutionContext<'a>,
    ) {
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
                texture,
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
                    incident,
                    normal: normal.direction,
                    texture,
                };
                let c = exe.run(color, &context).value;
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

struct KdPoint(Point3<f32>);

impl kd_tree::Point for KdPoint {
    type Dim = Dim3;

    fn get(&self, axis: Dim3) -> f32 {
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
    probability: f32,
}

impl<'a> LightBounce<'a> {
    fn accumulate_light(&self, samples: &mut [(Sample, f32)], exe: &mut ExecutionContext<'a>) {
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
                texture,
                probability,
                ..
            } = &hit.bounce;

            for &mut (ref mut sample, ref mut reflectance) in &mut *samples {
                let context = RenderContext {
                    wavelength: sample.wavelength,
                    incident,
                    normal: normal.direction,
                    texture,
                };

                let c = exe.run(color, &context).value * probability;

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

struct LightSource<'a> {
    surface: Surface<'a>,
    weight: f32,
}

impl<'a> kd_tree::Element for Arc<LightBounce<'a>> {
    type Point = KdPoint;

    fn position(&self) -> KdPoint {
        KdPoint(self.bounce.normal.origin)
    }

    fn sq_distance(&self, point: &KdPoint) -> f32 {
        (self.bounce.normal.origin - point.0).magnitude2()
    }
}
