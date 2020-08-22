use std::cmp::Ordering;

use cgmath::{EuclideanSpace, InnerSpace, Point2, Vector2};

use rand::Rng;

use crate::{
    cameras::Camera,
    film::{Area, Sample},
    program::ExecutionContext,
    tracer::{self, Bounce, RenderContext},
};

pub(crate) fn contribute<'a>(
    bounce: &Bounce<'a>,
    main_sample: &mut (Sample, f32),
    additional_samples: &mut [(Sample, f32)],
    exe: &mut ExecutionContext<'a>,
) {
    let &Bounce {
        ref ty,
        dispersed: _,
        color,
        incident,
        position: _,
        normal,
        texture,
        probability,
        ref direct_light,
    } = bounce;

    if ty.is_emission() {
        let initial_input = RenderContext {
            wavelength: main_sample.0.wavelength,
            incident,
            normal,
            texture,
        };
        let mut exe = color.memoize(initial_input, exe);

        main_sample.0.brightness += exe.run() * probability * main_sample.1;

        for &mut (ref mut sample, reflectance) in additional_samples {
            exe.update_input().set_wavelength(sample.wavelength);
            sample.brightness += exe.run() * probability * reflectance;
        }
    } else {
        {
            let initial_input = RenderContext {
                wavelength: main_sample.0.wavelength,
                incident,
                normal,
                texture,
            };
            let mut exe = color.memoize(initial_input, exe);

            main_sample.1 *= exe.run() * probability;

            for (sample, reflectance) in &mut *additional_samples {
                exe.update_input().set_wavelength(sample.wavelength);
                *reflectance *= exe.run() * probability;
            }
        }

        for direct in direct_light {
            let &tracer::DirectLight {
                dispersed: l_dispersed,
                color: l_color,
                incident: l_incident,
                normal: l_normal,
                probability: l_probability,
                texture: l_texture,
            } = direct;

            let initial_input = RenderContext {
                wavelength: main_sample.0.wavelength,
                incident: l_incident,
                normal: l_normal,
                texture: l_texture,
            };
            let mut exe = l_color.memoize(initial_input, exe);

            main_sample.0.brightness += exe.run() * l_probability * main_sample.1;

            if !l_dispersed {
                for &mut (ref mut sample, reflectance) in &mut *additional_samples {
                    exe.update_input().set_wavelength(sample.wavelength);
                    sample.brightness += exe.run() * l_probability * reflectance;
                }
            }
        }

        let brdf = ty.brdf(incident, normal);
        main_sample.1 *= brdf;

        for (_, reflectance) in additional_samples {
            *reflectance *= brdf;
        }
    }
}

pub struct Tile {
    pub area: Area<f32>,
    pub width: usize,
    pub height: usize,
}

impl Tile {
    pub fn area(&self) -> usize {
        self.width * self.height
    }

    pub fn sample_point<R: Rng>(&self, rng: &mut R) -> Point2<f32> {
        let offset = Vector2::new(
            self.area.size.x * rng.gen::<f32>(),
            self.area.size.y * rng.gen::<f32>(),
        );
        self.area.from + offset
    }
}

impl PartialEq for Tile {
    fn eq(&self, other: &Tile) -> bool {
        self.area
            .center()
            .to_vec()
            .magnitude2()
            .eq(&other.area.center().to_vec().magnitude2())
    }
}

impl PartialOrd for Tile {
    fn partial_cmp(&self, other: &Tile) -> Option<Ordering> {
        let ord = self
            .area
            .center()
            .to_vec()
            .magnitude2()
            .partial_cmp(&other.area.center().to_vec().magnitude2())
            .unwrap_or(Ordering::Equal);
        Some(ord)
    }
}

impl Ord for Tile {
    fn cmp(&self, other: &Tile) -> Ordering {
        self.partial_cmp(other).unwrap()
    }
}
impl Eq for Tile {}

pub(crate) fn make_tiles(
    film_width: usize,
    film_height: usize,
    tile_size: usize,
    camera: &Camera,
) -> Vec<Tile> {
    let mut tiles_x = film_width / tile_size;
    if tiles_x * tile_size < film_width {
        tiles_x += 1;
    }

    let mut tiles_y = film_height / tile_size;
    if tiles_y * tile_size < film_height {
        tiles_y += 1;
    }

    let mut tiles = Vec::with_capacity(tiles_x * tiles_y);

    for y in 0..tiles_y {
        for x in 0..tiles_x {
            let start = Point2::new(x * tile_size, y * tile_size);
            let size = Vector2::new(
                (film_width - start.x).min(tile_size),
                (film_height - start.y).min(tile_size),
            );
            tiles.push(Tile {
                area: camera.to_view_area(&Area::new(start, size), film_width, film_height),
                width: size.x,
                height: size.y,
            });
        }
    }

    tiles.sort();

    tiles
}
