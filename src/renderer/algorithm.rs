use std::cmp::Ordering::Equal;
use std::cmp::min;

use rand::{self, Rng, XorShiftRng};

use cgmath::{Vector, EuclideanVector, Vector2};

use tracer::{self, Bounce, RenderContext};
use cameras;
use world;

use renderer::Renderer;
use renderer::tile::{Tile, Sample, Area};

pub enum Algorithm {
    Simple {tile_size: u32}
}

impl Algorithm {
    pub fn make_tiles(&self, camera: &cameras::Camera, image_size: &Vector2<u32>, spectrum_bins: usize, (spectrum_min, spectrum_max): (f64, f64)) -> Vec<Tile> {
        match *self {
            Algorithm::Simple {tile_size, ..} => {
                let tiles_x = (image_size.x as f32 / tile_size as f32).ceil() as u32;
                let tiles_y = (image_size.y as f32 / tile_size as f32).ceil() as u32;

                let mut tiles = Vec::new();

                for y in 0..tiles_y {
                    for x in 0..tiles_x {
                        let from = Vector2::new(x * tile_size, y * tile_size);
                        let size = Vector2::new(min(image_size.x - from.x, tile_size), min(image_size.y - from.y, tile_size));

                        let image_area = Area::new(from, size);
                        let camera_area = camera.to_view_area(&image_area, image_size);

                        tiles.push(Tile::new(image_area, camera_area, spectrum_min, spectrum_max, spectrum_bins));
                    }
                }

                tiles.sort_by(|a, b| {
                    let a = Vector2::new(a.screen_area().from.x as f32, a.screen_area().from.y as f32);
                    let b = Vector2::new(b.screen_area().from.x as f32, b.screen_area().from.y as f32);
                    let half_size = Vector2::new(image_size.x as f32 / 2.0, image_size.y as f32 / 2.0);
                    a.sub_v(&half_size).length2().partial_cmp(&b.sub_v(&half_size).length2()).unwrap_or(Equal)
                });
                tiles
            }
        }
    }

    pub fn render_tile(&self, tile: &mut Tile, camera: &cameras::Camera, world: &world::World, renderer: &Renderer) {
        let rng: XorShiftRng = rand::thread_rng().gen();

        match *self {
            Algorithm::Simple {..} => {
                simple(rng, tile, camera, world, renderer);
            }
        }
    }
}

fn contribute(bounce: &Bounce, sample: &mut Sample, reflectance: &mut f64, require_white: bool) -> bool {
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

    if let tracer::BounceType::Emission = *ty {
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

pub fn simple<R: Rng>(mut rng: R, tile: &mut Tile, camera: &cameras::Camera, world: &world::World, renderer: &Renderer) {
    for _ in 0..(tile.pixel_count() * renderer.pixel_samples as usize) {
        let position = tile.sample_position(&mut rng);

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

        tile.expose(main_sample.0, position);

        if used_additional {
            for (sample, _) in additional_samples {
                tile.expose(sample, position);
            }
        }
    }
}