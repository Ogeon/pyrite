use std;

use rand::{self, Rng, XorShiftRng};

use cgmath::{Vector, EuclideanVector, Vector3};
use cgmath::Point;
use cgmath::{Ray3};

use tracer::{self, Bounce, BounceType, RenderContext};
use cameras;
use world::World;
use lamp;

use renderer::Renderer;
use film::{Tile, Sample};
use utils::pairs;

/*impl Algorithm {
    pub fn render_tile(&self, tile: &mut Tile, camera: &cameras::Camera, world: &World, renderer: &Renderer) {
        let rng: XorShiftRng = rand::thread_rng().gen();

        match *self {
            Algorithm::Simple {..} => simple(rng, tile, camera, world, renderer),
            Algorithm::Bidirectional { ref params, .. } => bidirectional(rng, tile, camera, world, renderer, params)
        }
    }
}*/

pub fn contribute(bounce: &Bounce, sample: &mut Sample, reflectance: &mut f64, require_white: bool) -> bool {
    let &Bounce {
        ref ty,
        ref light,
        color,
        incident,
        normal,
        probability,
        ref direct_light,
    } = bounce;

    if !light.is_white() && require_white {
        return false;
    }

    let context = RenderContext {
        wavelength: sample.wavelength,
        incident: incident,
        normal: normal.direction
    };

    let c = color.get(&context) * probability;

    if let BounceType::Emission = *ty {
        sample.brightness += c * *reflectance;
    } else {
        *reflectance *= c;

        for direct in direct_light {
            let &tracer::DirectLight {
                light: ref l_light,
                color: l_color,
                incident: l_incident,
                normal: l_normal,
                probability: l_probability,
            } = direct;

            if l_light.is_white() || !require_white {
                let context = RenderContext {
                    wavelength: sample.wavelength,
                    incident: l_incident,
                    normal: l_normal
                };

                let l_c = l_color.get(&context) * l_probability;
                sample.brightness += l_c * *reflectance;
            }
        }

        *reflectance *= ty.brdf(&incident, &normal.direction);
    }

    true
}

pub fn simple<R: Rng>(mut rng: R, tile: &mut Tile, camera: &cameras::Camera, world: &World, renderer: &Renderer) {
    for _ in 0..(tile.area() * renderer.pixel_samples as usize) {
        let position = tile.sample_point(&mut rng);

        let ray = camera.ray_towards(&position, &mut rng);
        let wavelength = tile.sample_wavelength(&mut rng);
        let light = tracer::Light::new(wavelength);
        let path = tracer::trace(&mut rng, ray, light, world, renderer.bounces, renderer.light_samples);

        let mut main_sample = (Sample {
            wavelength: wavelength,
            brightness: 0.0,
            weight: 1.0
        }, 1.0);

        let mut used_additional = true;
        let mut additional_samples: Vec<_> = (0..renderer.spectrum_samples-1).map(|_| (Sample {
            wavelength: tile.sample_wavelength(&mut rng),
            brightness: 0.0,
            weight: 1.0,
        }, 1.0)).collect();

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

pub struct BidirParams {
    pub bounces: u32
}

pub fn bidirectional<R: Rng>(mut rng: R, tile: &mut Tile, camera: &cameras::Camera, world: &World, renderer: &Renderer, bidir_params: &BidirParams) {
    for _ in 0..(tile.area() * renderer.pixel_samples as usize) {
        let position = tile.sample_point(&mut rng);
        let wavelength = tile.sample_wavelength(&mut rng);
        let light = tracer::Light::new(wavelength);

        let camera_ray = camera.ray_towards(&position, &mut rng);
        let lamp_sample = world.pick_lamp(&mut rng).and_then(|(l, p)| l.sample_ray(&mut rng).map(|r| (r, p)));
        let lamp_path = if let Some((lamp_sample, probability)) = lamp_sample {
            let lamp::RaySample { mut ray, surface, weight } = lamp_sample;
            
            let mut light = light.clone();
            let (color, normal) = match surface {
                lamp::Surface::Physical { normal, material } => {
                    let color = material.get_emission(&mut light, &-ray.direction, &normal, &mut rng);
                    (color, normal)
                },
                lamp::Surface::Color(color) => (Some(color), ray)
            };
            ray.origin.add_self_v(&normal.direction.mul_s(0.00001));


            if let Some(color) = color {
                let mut path = Vec::with_capacity(bidir_params.bounces as usize + 1);
                path.push(Bounce {
                    ty: BounceType::Emission,
                    light: light.clone(),
                    color: color,
                    incident: Vector3::new(0.0, 0.0, 0.0),
                    normal: normal,
                    probability: weight / probability,
                    direct_light: vec![]
                });

                path.extend(tracer::trace(&mut rng, ray, light, world, bidir_params.bounces, 0));

                pairs(&mut path, |to, from| {
                    to.incident = -from.incident;
                    if let BounceType::Diffuse(_, ref mut o) = from.ty {
                        *o = from.incident
                    }
                });

                if path.len() > 1 {
                    if let Some(last) = path.pop() {
                        match last.ty {
                            BounceType::Diffuse(_, _) | BounceType::Specular => path.push(last),
                            BounceType::Emission => {}
                        }
                    }
                }
                path.reverse();
                path
            } else {
                vec![]
            }
        } else {
            vec![]
        };


        let camera_path = tracer::trace(&mut rng, camera_ray, light, world, renderer.bounces, renderer.light_samples);

        let total = (camera_path.len() * lamp_path.len()) as f64;
        let weight = 1.0 / total;

        let mut main_sample = (Sample {
            wavelength: wavelength,
            brightness: 0.0,
            weight: 1.0
        }, 1.0);

        let mut used_additional = true;
        let mut additional_samples: Vec<_> = (0..renderer.spectrum_samples-1).map(|_| (Sample {
            wavelength: tile.sample_wavelength(&mut rng),
            brightness: 0.0,
            weight: 1.0,
        }, 1.0)).collect();

        for bounce in camera_path {
            for &mut (ref mut sample, ref mut reflectance) in &mut additional_samples {
                used_additional = contribute(&bounce, sample, reflectance, true) && used_additional;
            }

            {
                let (ref mut sample, ref mut reflectance) = main_sample;
                contribute(&bounce, sample, reflectance, false);
            }

            for mut contribution in connect_paths(&bounce, &main_sample, &additional_samples, &lamp_path, world, used_additional) {
                contribution.weight = weight;
                tile.expose(position, contribution);
            }
        }

        tile.expose(position, main_sample.0.clone());

        if used_additional {
            for &(ref sample, _) in &additional_samples {
                tile.expose(position, sample.clone());
            }
        }

        let weight = 1.0 / lamp_path.len() as f64;
        for (i, bounce) in lamp_path.iter().enumerate() {
            if let BounceType::Diffuse(_, _) = bounce.ty {

            } else {
                continue;
            }

            let camera_hit = camera.is_visible(&bounce.normal.origin, &world, &mut rng);
            if let Some((position, ray)) = camera_hit {
                if position.x > -1.0 && position.x < 1.0 && position.y > -1.0 && position.y < 1.0 {
                    let sq_distance = ray.origin.sub_p(&bounce.normal.origin).length2();
                    let scale = 1.0 / (sq_distance);
                    let brdf_in = bounce.ty.brdf(&-ray.direction, &bounce.normal.direction) / bounce.ty.brdf(&bounce.incident, &bounce.normal.direction);

                    main_sample.0.brightness = 0.0;
                    main_sample.0.weight = weight;
                    main_sample.1 = scale;

                    used_additional = true;
                    for &mut(ref mut sample, ref mut reflectance) in &mut additional_samples {
                        sample.brightness = 0.0;
                        sample.weight = weight;
                        *reflectance = scale;
                    }

                    for (i, bounce) in lamp_path[i..].iter().enumerate() {
                        for &mut (ref mut sample, ref mut reflectance) in &mut additional_samples {
                            used_additional = contribute(bounce, sample, reflectance, true) && used_additional;
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

                    tile.expose(position, main_sample.0.clone());

                    if used_additional {
                        for &(ref sample, _) in &additional_samples {
                            tile.expose(position, sample.clone());
                        }
                    }
                }
            }
        }
    }
}

fn connect_paths(bounce: &Bounce, main: &(Sample, f64), additional: &[(Sample, f64)], path: &[Bounce], world: &World, use_additional: bool) -> Vec<Sample> {
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

        let direction = to.sub_p(&from);
        let ray = Ray3::new(from, direction.normalize());
        let sq_distance = direction.length2();

        if bounce.normal.direction.dot(&ray.direction) <= 0.0 {
            continue;
        }

        if lamp_bounce.normal.direction.dot(& -ray.direction) <= 0.0 {
            continue;
        }

        let hit = world.intersect(&ray).map(|(hit_normal, _)| hit_normal.origin.sub_p(&from).length2());
        if let Some(dist) = hit {
            if dist < sq_distance - 0.0000001 {
                continue;
            }
        }

        let cos_out = bounce.normal.direction.dot(&ray.direction).abs();
        let cos_in = lamp_bounce.normal.direction.dot(& -ray.direction).abs();
        let brdf_out = bounce_brdf(&bounce.incident, &bounce.normal.direction, &ray.direction) / bounce.ty.brdf(&bounce.incident, &bounce.normal.direction);

        let scale = cos_in * cos_out * brdf_out / (2.0 * std::f64::consts::PI * sq_distance);
        let brdf_in = lamp_bounce.ty.brdf(&-ray.direction, &lamp_bounce.normal.direction) / lamp_bounce.ty.brdf(&lamp_bounce.incident, &lamp_bounce.normal.direction);

        let mut use_additional = use_additional;
        let mut additional: Vec<_> = additional.iter().cloned().map(|(s, r)| (s, r*scale)).collect();
        let mut main = main.clone();
        main.1 *= scale;

        for (i, bounce) in path[i..].iter().enumerate() {
            for &mut(ref mut sample, ref mut reflectance) in &mut additional {
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
