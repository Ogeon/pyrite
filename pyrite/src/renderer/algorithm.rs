use std::cmp::Ordering;

use cgmath::{EuclideanSpace, InnerSpace, Point2, Vector2};

use rand::Rng;

use crate::cameras::Camera;
use crate::film::{Area, Sample};
use crate::{
    program::ExecutionContext,
    tracer::{self, Bounce, BounceType, RenderContext},
};

pub(crate) fn contribute<'a>(
    bounce: &Bounce<'a>,
    sample: &mut Sample,
    reflectance: &mut f32,
    require_white: bool,
    exe: &mut ExecutionContext<'a>,
) -> bool {
    let &Bounce {
        ref ty,
        dispersed,
        color,
        incident,
        position: _,
        normal,
        texture,
        probability,
        ref direct_light,
    } = bounce;

    if dispersed && require_white {
        return false;
    }

    let context = RenderContext {
        wavelength: sample.wavelength,
        incident,
        normal,
        texture,
    };

    let c = exe.run(color, &context) * probability;

    if let BounceType::Emission = *ty {
        sample.brightness += c * *reflectance;
    } else {
        *reflectance *= c;

        for direct in direct_light {
            let &tracer::DirectLight {
                dispersed: l_dispersed,
                color: l_color,
                incident: l_incident,
                normal: l_normal,
                probability: l_probability,
            } = direct;

            if !l_dispersed || !require_white {
                let context = RenderContext {
                    wavelength: sample.wavelength,
                    incident: l_incident,
                    normal: l_normal,
                    texture,
                };

                let l_c = exe.run(l_color, &context) * l_probability;
                sample.brightness += l_c * *reflectance;
            }
        }

        *reflectance *= ty.brdf(incident, normal);
    }

    true
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
