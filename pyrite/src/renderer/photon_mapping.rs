use std::{cell::Cell, sync::Arc};

use rand::{self, Rng, SeedableRng};
use rand_xorshift::XorShiftRng;

use cgmath::{EuclideanSpace, InnerSpace, Point2, Point3, Vector3};

use super::{
    algorithm::{contribute, make_tiles},
    Progress, Renderer, TaskRunner,
};
use crate::cameras::Camera;
use crate::film::{DetachedPixel, Film, Sample};
use crate::lamp::Surface;
use crate::spatial::kd_tree::{self, KdTree};
use crate::spatial::Dim3;
use crate::tracer::{trace, Bounce, BounceType, RenderContext};
use crate::utils::{pairs, BatchRange};
use crate::{
    materials::ProbabilityInput,
    math::DIST_EPSILON,
    program::{ExecutionContext, Resources},
    world::World,
};

pub(crate) fn render<F: FnMut(Progress<'_>)>(
    film: &Film,
    task_runner: TaskRunner,
    mut on_status: F,
    renderer: &Renderer,
    config: &Config,
    world: &World,
    camera: &Camera,
    resources: &Resources,
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
        on_status(Progress {
            progress: 0,
            message: &status_message,
        });
        progress = 0;
        task_runner.run_tasks(
            tiles.iter().map(|f| (f, gen_rng())),
            |_index, (tile, mut rng), _progress| {
                let mut all_bounces = vec![];
                let mut bounces = Vec::with_capacity(renderer.bounces as usize);
                let mut additional_samples =
                    Vec::with_capacity(renderer.spectrum_samples as usize - 1);
                let mut exe = ExecutionContext::new(resources);

                for _ in 0..tile.area() as usize {
                    bounces.clear();
                    additional_samples.clear();

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
                        additional_samples.swap_remove(rng.gen_range(0..additional_samples.len()));
                    let wavelength = main_sample.0.wavelength;

                    trace(
                        &mut bounces,
                        &mut rng,
                        ray,
                        wavelength,
                        world,
                        renderer.bounces,
                        renderer.light_samples,
                        &mut exe,
                    );
                    let p = 1.0 / renderer.bounces as f32;

                    let mut use_additional = true;

                    let mut current = Parent::Source(position);
                    for bounce in bounces.drain(..) {
                        use_additional = !bounce.dispersed && use_additional;
                        let additional_samples = if use_additional {
                            &mut *additional_samples
                        } else {
                            &mut []
                        };

                        contribute(&bounce, &mut main_sample, additional_samples, &mut exe);

                        match bounce.ty {
                            BounceType::Diffuse(_, _) => {
                                let b = Arc::new(CameraBounce {
                                    wavelength,
                                    parent: current,
                                    bounce,
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
                                    wavelength,
                                    parent: current,
                                    bounce,
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

                    film.expose(position, main_sample.0);
                    if use_additional {
                        for (sample, _) in additional_samples.drain(..) {
                            film.expose(position, sample);
                        }
                    }
                }
                all_bounces
            },
            |_i, bounces| {
                camera_bounces.extend(bounces);
                progress += 1;
                on_status(Progress {
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
            on_status(Progress {
                progress: 0,
                message: &status_message,
            });
            progress = 0;
            task_runner.run_tasks(
                BatchRange::new(0..config.photons, 5000).map(|batch| {
                    let rng: XorShiftRng = gen_rng();
                    (batch, rng)
                }),
                |_index, (num_rays, mut rng), _progress| {
                    let mut processed = vec![];
                    let mut bounces = Vec::with_capacity(renderer.bounces as usize);
                    let mut exe = ExecutionContext::new(resources);

                    for _ in 0..num_rays {
                        bounces.clear();

                        let res = world
                            .pick_lamp(&mut rng)
                            .and_then(|(lamp, p)| lamp.sample_ray(&mut rng).map(|s| (lamp, p, s)));

                        if let Some((_lamp, probability, mut ray_sample)) = res {
                            let wavelength = film.sample_wavelength(&mut rng);

                            let (color, material_probability, dispersed, normal, texture) =
                                match ray_sample.surface {
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
                                            incident: -ray_sample.ray.direction,
                                            texture_coordinate: texture,
                                        };

                                        let probability =
                                            component.get_probability(&mut exe, &input);

                                        (
                                            component.bsdf.color,
                                            probability,
                                            input.wavelength_used.get(),
                                            normal,
                                            texture,
                                        )
                                    }
                                    Surface::Color(color) => (
                                        color,
                                        1.0,
                                        false,
                                        ray_sample.ray.direction,
                                        Point2::origin(),
                                    ),
                                };

                            ray_sample.ray.origin += normal * DIST_EPSILON;

                            trace(
                                &mut bounces,
                                &mut rng,
                                ray_sample.ray,
                                wavelength,
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
                                wavelength,
                                bounce: Bounce {
                                    ty: BounceType::Emission,
                                    dispersed,
                                    color,
                                    incident,
                                    position: ray_sample.ray.origin,
                                    normal,
                                    texture,
                                    probability: ray_sample.weight
                                        * probability
                                        * material_probability,
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
                                            wavelength,
                                            bounce,
                                            probability: p,
                                        });
                                        current = b.clone();
                                        processed.push(b);
                                    }
                                    BounceType::Specular => {
                                        let b = Arc::new(LightBounce {
                                            parent: Some(current),
                                            wavelength,
                                            bounce,
                                            probability: p,
                                        });
                                        current = b.clone();
                                    }
                                    BounceType::Emission => break,
                                }
                            }
                        }
                    }

                    (num_rays, processed)
                },
                |_, (n, bounces)| {
                    light_bounces.extend(bounces);
                    progress += n;
                    on_status(Progress {
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
            on_status(Progress {
                progress: 0,
                message: &status_message,
            });
            progress = 0;

            task_runner.run_tasks(
                camera_bounces.chunks(5000).map(|b| (b, gen_rng())),
                |_index, (bounces, mut rng), _progress| {
                    let mut exe = ExecutionContext::new(resources);

                    for hit in bounces {
                        let pixel = &hit.pixel;
                        let point = KdPoint(hit.bounce.position);
                        let neighbors: Vec<_> =
                            light_bounces.neighbors(&point, config.radius).collect();
                        let num_neighbors = neighbors.len();
                        for neighbor in neighbors {
                            let bounce_dispersed = hit.bounce.dispersed;
                            let neighbor_dispersed = neighbor.bounce.dispersed;

                            if !bounce_dispersed || !neighbor_dispersed {
                                let (use_additional, wavelength) =
                                    if !bounce_dispersed && !neighbor_dispersed {
                                        (true, neighbor.wavelength)
                                    } else if !bounce_dispersed {
                                        (false, neighbor.wavelength)
                                    } else {
                                        (false, hit.wavelength)
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

                                let mut weight = incident.dot(hit.bounce.normal).max(0.0);
                                if weight > 0.0 {
                                    weight *= hit.bounce.incident.dot(-hit.bounce.normal).max(0.0);
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
                    on_status(Progress {
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
    wavelength: f32,
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
                dispersed: _,
                color,
                incident,
                normal,
                texture,
                probability,
                ..
            } = &hit.bounce;

            let brdf = if let Some((brdf, ray_out)) = first_brdf.take() {
                brdf(incident, normal, ray_out)
            } else {
                ty.brdf(incident, normal)
            };

            for &mut (ref sample, ref mut reflectance) in &mut *samples {
                let context = RenderContext {
                    wavelength: sample.wavelength,
                    incident,
                    normal: normal,
                    texture,
                };
                let c = exe.run(color, &context);
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
    wavelength: f32,
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
                dispersed: _,
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
                    normal,
                    texture,
                };

                let c = exe.run(color, &context) * probability;

                if let BounceType::Emission = *ty {
                    sample.brightness = c * *reflectance;
                } else {
                    *reflectance *= c * ty.brdf(incident, normal);
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
        KdPoint(self.bounce.position)
    }

    fn sq_distance(&self, point: &KdPoint) -> f32 {
        (self.bounce.position - point.0).magnitude2()
    }
}
