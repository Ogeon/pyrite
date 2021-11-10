use std::{cmp::Ordering, usize};

use cgmath::{EuclideanSpace, InnerSpace, Point2, Vector2};

use crate::{cameras::Camera, film::Area};

use super::samplers::Sampler;

#[derive(Clone)]
pub struct Tile {
    pub area: Area<f32>,
    pub width: usize,
    pub height: usize,
}

impl Tile {
    pub fn area(&self) -> usize {
        self.width * self.height
    }

    pub(crate) fn sample_point(&self, rng: &mut dyn Sampler) -> Point2<f32> {
        self.area.sample_point(rng)
    }

    pub(crate) fn pixels(&self) -> Pixels {
        Pixels {
            origin: self.area.from,
            width: self.width,
            height: self.height,
            x: 0,
            y: 0,
            pixel_size: self.area.size.x / self.width as f32,
        }
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

pub(crate) struct Pixels {
    origin: Point2<f32>,
    width: usize,
    height: usize,
    x: usize,
    y: usize,
    pixel_size: f32,
}

impl Iterator for Pixels {
    type Item = Area<f32>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.y >= self.height {
            return None;
        }

        let from = self.origin + Vector2::new(self.x as f32, self.y as f32) * self.pixel_size;

        self.x += 1;
        if self.x >= self.width {
            self.x = 0;
            self.y += 1;
        }

        Some(Area {
            from,
            size: Vector2::new(self.pixel_size, self.pixel_size),
        })
    }
}
