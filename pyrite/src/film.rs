use std::cmp::{min, Eq, Ord, Ordering, PartialEq, PartialOrd};
use std::collections::hash_map::{self, Entry, HashMap};
use std::iter::Enumerate;
use std::ops::{Drop, Sub};
use std::slice::Iter;
use std::sync::atomic::{self, AtomicBool};
use std::sync::{RwLock, RwLockReadGuard, RwLockWriteGuard};

use rand::Rng;

use cgmath::{BaseNum, EuclideanSpace, InnerSpace, Point2, Vector2};

use crate::cameras::Camera;

pub struct Film {
    width: usize,
    tile_size: usize,
    tiles_per_row: usize,
    wavelength_start: f64,
    wavelength_width: f64,
    bins: usize,
    bin_ratio: f64,
    aspect: AspectRatio,
    pixels: Vec<FilmTile>,
    tiles: Vec<OrderTile>,
}

impl Film {
    pub fn new(
        width: usize,
        height: usize,
        tile_size: usize,
        wavelength_span: (f64, f64),
        bins: usize,
        camera: &Camera,
    ) -> Film {
        let mut tiles_x = width / tile_size;
        if tiles_x * tile_size < width {
            tiles_x += 1;
        }

        let mut tiles_y = height / tile_size;
        if tiles_y * tile_size < height {
            tiles_y += 1;
        }

        let mut pixels = Vec::with_capacity(tiles_x * tiles_y);
        let mut tiles = Vec::with_capacity(tiles_x * tiles_y);
        for y in 0..tiles_y {
            for x in 0..tiles_x {
                let start = Point2::new(x * tile_size, y * tile_size);
                let size = Vector2::new(
                    min(width - start.x, tile_size),
                    min(height - start.y, tile_size),
                );
                pixels.push(FilmTile::new(size.x, size.y, bins));
                tiles.push(OrderTile {
                    index: x + y * tiles_x,
                    area: camera.to_view_area(&Area::new(start, size), width, height),
                    width: size.x,
                    height: size.y,
                });
            }
        }
        pixels.shrink_to_fit();
        tiles.shrink_to_fit();
        tiles.sort();

        let wl_width = wavelength_span.1 - wavelength_span.0;
        Film {
            width: width,
            tile_size: tile_size,
            tiles_per_row: tiles_x,
            wavelength_start: wavelength_span.0,
            wavelength_width: wl_width,
            bins: bins,
            bin_ratio: bins as f64 / wl_width,
            aspect: AspectRatio::new(width, height),
            pixels: pixels,
            tiles: tiles,
        }
    }

    pub fn merge_pixels<'a, I>(&self, pixels: I)
    where
        I: IntoIterator<Item = (Point2<f64>, Pixel)>,
    {
        self.merge_pixels_internal(
            pixels
                .into_iter()
                .filter_map(|(pos, pixel)| self.aspect.to_pixel(&pos).map(|p| (p, pixel))),
        );
    }

    fn merge_pixels_internal<'a, I>(&self, pixels: I)
    where
        I: IntoIterator<Item = (Point2<usize>, Pixel)>,
    {
        let mut pixels: Vec<_> = pixels
            .into_iter()
            .map(|(pos, pixel)| {
                let x = pos.x / self.tile_size;
                let y = pos.y / self.tile_size;
                let index = x + y * self.tiles_per_row;
                let tile_x = pos.x - x * self.tile_size;
                let tile_y = pos.y - y * self.tile_size;

                (index, Point2::new(tile_x, tile_y), pixel)
            })
            .collect();
        pixels.sort_by(|&(i1, _, _), &(i2, _, _)| i1.cmp(&i2));
        let mut pixels = pixels.into_iter();
        let mut sample = pixels.next();

        while let Some(current_index) = sample.as_ref().map(|&(index, _, _)| index) {
            let tile = &self.pixels[current_index];
            let mut tile_pixels = tile.pixels_mut();
            let tile_w = tile.width;

            while let Some((index, position, pixel)) = sample.as_ref().cloned() {
                if index == current_index {
                    tile_pixels[position.x + position.y * tile_w].merge(&pixel);
                    sample = pixels.next();
                } else {
                    break;
                }
            }
        }
    }

    pub fn with_changed_pixels<F>(&self, mut action: F)
    where
        F: FnMut(Point2<usize>, Spectrum),
    {
        let mut start_pos = Point2::origin();

        for tile in &self.pixels {
            if let Some(pixels) = tile.read_if_changed() {
                for (i, pixel) in pixels.iter().enumerate() {
                    let x = i % tile.width;
                    let y = i / tile.width;
                    let pos = start_pos + Vector2::new(x, y);
                    action(
                        pos,
                        Spectrum {
                            min: self.wavelength_start,
                            width: self.wavelength_width,
                            values: pixel.final_values(),
                        },
                    );
                }
            }

            let next_x = start_pos.x + tile.width;
            start_pos = if next_x >= self.width {
                Point2::new(0, start_pos.y + self.tile_size)
            } else {
                Point2::new(next_x, start_pos.y)
            };
        }
    }

    pub fn num_tiles(&self) -> usize {
        self.tiles.len()
    }

    pub fn sample_wavelength<R: Rng>(&self, rng: &mut R) -> f64 {
        self.wavelength_start + self.wavelength_width * rng.gen::<f64>()
    }

    pub fn new_pixel(&self) -> Pixel {
        Pixel::new(self.bins)
    }

    pub fn to_pixel_sample(&self, sample: &Sample) -> PixelSample {
        PixelSample {
            value: sample.brightness,
            weight: sample.weight,
            bin: self.wavelength_to_bin(sample.wavelength),
        }
    }

    fn expose_tile(&self, tile: &OrderTile, pixels: Vec<Pixel>) {
        let mut film = self.pixels[tile.index].pixels_mut();

        for (src, dest) in pixels.into_iter().zip(film.iter_mut()) {
            dest.merge(&src);
        }
    }

    fn wavelength_to_bin(&self, wavelength: f64) -> usize {
        ((wavelength - self.wavelength_start) * self.bin_ratio) as usize
    }
}

impl<'a> IntoIterator for &'a Film {
    type IntoIter = Tiles<'a>;
    type Item = Tile<'a>;

    fn into_iter(self) -> Tiles<'a> {
        Tiles {
            film: self,
            tiles: self.tiles.iter(),
        }
    }
}

struct FilmTile {
    changed: AtomicBool,
    width: usize,
    pixels: RwLock<Vec<Pixel>>,
}

impl FilmTile {
    fn new(width: usize, height: usize, bins: usize) -> FilmTile {
        let mut pixels = vec![Pixel::new(bins); width * height];
        pixels.shrink_to_fit();
        FilmTile {
            changed: AtomicBool::new(false),
            width: width,
            pixels: RwLock::new(pixels),
        }
    }

    fn pixels_mut(&self) -> RwLockWriteGuard<'_, Vec<Pixel>> {
        self.changed.store(true, atomic::Ordering::Release);
        self.pixels
            .write()
            .ok()
            .expect("could not lock pixels for writing")
    }

    fn read_if_changed(&self) -> Option<RwLockReadGuard<'_, Vec<Pixel>>> {
        if self.changed.swap(false, atomic::Ordering::AcqRel) {
            Some(
                self.pixels
                    .read()
                    .ok()
                    .expect("could not lock pixels for reading"),
            )
        } else {
            None
        }
    }
}

#[derive(Clone)]
pub struct Pixel {
    spectrum: Vec<(f64, f64)>,
}

impl Pixel {
    pub fn new(steps: usize) -> Pixel {
        Pixel {
            spectrum: vec![(0.0, 0.0); steps],
        }
    }

    pub fn final_values(&self) -> Vec<f64> {
        self.spectrum
            .iter()
            .map(|&(b, w)| if w > 0.0 { b / w } else { 0.0 })
            .collect()
    }

    pub fn merge(&mut self, other: &Pixel) {
        for (&mut (ref mut self_sum, ref mut self_weight), &(other_sum, other_weight)) in
            &mut self.spectrum.iter_mut().zip(&other.spectrum)
        {
            *self_sum += other_sum;
            *self_weight += other_weight;
        }
    }

    pub fn add_sample(&mut self, sample: PixelSample) {
        let (ref mut sum, ref mut weight) = self.spectrum[sample.bin];
        *sum += sample.value * sample.weight;
        *weight += sample.weight;
    }
}

#[derive(Clone)]
pub struct PixelSample {
    value: f64,
    weight: f64,
    bin: usize,
}

struct AspectRatio {
    size: f64,
    ratio: f64,
    orientation: Orientation,
}

impl AspectRatio {
    fn new(width: usize, height: usize) -> AspectRatio {
        if width >= height {
            AspectRatio {
                size: width as f64,
                ratio: height as f64 / width as f64,
                orientation: Orientation::Horizontal,
            }
        } else {
            AspectRatio {
                size: height as f64,
                ratio: width as f64 / height as f64,
                orientation: Orientation::Vertical,
            }
        }
    }

    fn contains(&self, point: &Point2<f64>) -> bool {
        match self.orientation {
            Orientation::Horizontal => point.y.abs() <= self.ratio,
            Orientation::Vertical => point.x.abs() <= self.ratio,
        }
    }

    fn to_pixel(&self, point: &Point2<f64>) -> Option<Point2<usize>> {
        if self.contains(&point) {
            let (x, y) = match self.orientation {
                Orientation::Horizontal => (point.x + 1.0, point.y + self.ratio),
                Orientation::Vertical => (point.x + self.ratio, point.y + 1.0),
            };
            Some(Point2::new(
                (self.size * x * 0.5) as usize,
                (self.size * y * 0.5) as usize,
            ))
        } else {
            None
        }
    }
}

enum Orientation {
    Horizontal,
    Vertical,
}

pub struct Area<S> {
    pub from: Point2<S>,
    pub size: Vector2<S>,
}

impl<S> Area<S> {
    pub fn new(from: Point2<S>, size: Vector2<S>) -> Area<S> {
        Area {
            from: from,
            size: size,
        }
    }

    pub fn center(&self) -> Point2<S>
    where
        S: BaseNum,
    {
        self.from + self.size / (S::one() + S::one())
    }

    pub fn contains(&self, point: &Point2<S>) -> bool
    where
        for<'a> &'a S: PartialOrd<&'a S>,
        for<'a> &'a S: Sub<&'a S, Output = S>,
    {
        &self.from.x <= &point.x
            && &self.size.x > &(&point.x - &self.from.x)
            && &self.from.y <= &point.y
            && &self.size.y > &(&point.y - &self.from.y)
    }
}

pub struct OrderTile {
    index: usize,
    area: Area<f64>,
    width: usize,
    height: usize,
}

impl PartialEq for OrderTile {
    fn eq(&self, other: &OrderTile) -> bool {
        self.area
            .center()
            .to_vec()
            .magnitude2()
            .eq(&other.area.center().to_vec().magnitude2())
    }
}

impl PartialOrd for OrderTile {
    fn partial_cmp(&self, other: &OrderTile) -> Option<Ordering> {
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

impl Ord for OrderTile {
    fn cmp(&self, other: &OrderTile) -> Ordering {
        self.partial_cmp(other).unwrap()
    }
}
impl Eq for OrderTile {}

#[derive(Clone)]
pub struct Sample {
    pub brightness: f64,
    pub wavelength: f64,
    pub weight: f64,
}

pub struct Tile<'a> {
    film: &'a Film,
    tile: &'a OrderTile,
    pixels: Vec<Pixel>,
    extra_buffer: LimitedMap,
}

impl<'a> Tile<'a> {
    pub fn sample_point<R: Rng>(&self, rng: &mut R) -> Point2<f64> {
        let offset = Vector2::new(
            self.tile.area.size.x * rng.gen::<f64>(),
            self.tile.area.size.y * rng.gen::<f64>(),
        );
        self.tile.area.from + offset
    }

    pub fn expose(&mut self, position: Point2<f64>, sample: Sample) {
        let bin = self.film.wavelength_to_bin(sample.wavelength);

        if self.tile.area.contains(&position) {
            let Vector2 { mut x, mut y } = position - self.tile.area.from;
            x = x * self.tile.width as f64 / self.tile.area.size.x;
            y = y * self.tile.height as f64 / self.tile.area.size.y;

            if let Some(pixel) = self
                .pixels
                .get_mut(x as usize + y as usize * self.tile.width)
            {
                pixel.add_sample(PixelSample {
                    value: sample.brightness,
                    weight: sample.weight,
                    bin: bin,
                });
            }
        } else if self.film.aspect.contains(&position) {
            if let Some(position) = self.film.aspect.to_pixel(&position) {
                self.extra_buffer.insert(
                    position,
                    PixelSample {
                        value: sample.brightness,
                        weight: sample.weight,
                        bin: bin,
                    },
                );

                if self.extra_buffer.is_full() {
                    self.film.merge_pixels_internal(self.extra_buffer.drain());
                }
            }
        }
    }

    pub fn merge_pixel(&mut self, position: &Point2<f64>, pixel: &Pixel) {
        if self.tile.area.contains(position) {
            let Vector2 { mut x, mut y } = position - self.tile.area.from;
            x = x * self.tile.width as f64 / self.tile.area.size.x;
            y = y * self.tile.height as f64 / self.tile.area.size.y;

            if let Some(p) = self
                .pixels
                .get_mut(x as usize + y as usize * self.tile.width)
            {
                p.merge(pixel);
            }
        }
    }

    pub fn area(&self) -> usize {
        self.tile.width * self.tile.height
    }

    pub fn sample_wavelength<R: Rng>(&self, rng: &mut R) -> f64 {
        self.film.sample_wavelength(rng)
    }

    pub fn index(&self) -> usize {
        self.tile.index
    }
}

impl<'a> Drop for Tile<'a> {
    fn drop(&mut self) {
        if !self.pixels.is_empty() {
            let mut pixels = vec![];
            ::std::mem::swap(&mut pixels, &mut self.pixels);
            self.film.expose_tile(self.tile, pixels);

            self.film.merge_pixels_internal(self.extra_buffer.drain());
        }
    }
}

pub struct Tiles<'a> {
    film: &'a Film,
    tiles: Iter<'a, OrderTile>,
}

impl<'a> Iterator for Tiles<'a> {
    type Item = Tile<'a>;

    fn next(&mut self) -> Option<Tile<'a>> {
        self.tiles.next().map(|t| Tile {
            film: self.film,
            tile: t,
            pixels: vec![Pixel::new(self.film.bins); t.width * t.height],
            extra_buffer: LimitedMap::new(4096, self.film.bins),
        })
    }
}

struct LimitedMap {
    limit: usize,
    bins: usize,
    map: HashMap<Point2<usize>, Pixel>,
}

impl LimitedMap {
    fn new(limit: usize, bins: usize) -> LimitedMap {
        LimitedMap {
            limit: limit,
            bins: bins,
            map: HashMap::new(),
        }
    }

    fn insert(&mut self, pos: Point2<usize>, sample: PixelSample) {
        let len = self.map.len();
        match self.map.entry(pos) {
            Entry::Vacant(e) => {
                if len < self.limit {
                    let mut p = Pixel::new(self.bins);
                    p.add_sample(sample);
                    e.insert(p);
                }
            }
            Entry::Occupied(mut e) => e.get_mut().add_sample(sample),
        }
    }

    fn drain(&mut self) -> hash_map::IntoIter<Point2<usize>, Pixel> {
        //TODO: Make this real
        let mut map = HashMap::with_capacity(self.map.len());
        ::std::mem::swap(&mut self.map, &mut map);
        map.into_iter()
    }

    fn is_full(&self) -> bool {
        self.map.len() >= self.limit
    }
}

pub struct Spectrum {
    pub min: f64,
    pub width: f64,
    values: Vec<f64>,
}

impl Spectrum {
    pub fn segments(&self) -> SpectrumSegments<'_> {
        SpectrumSegments {
            start: self.min,
            segment_width: self.width / self.values.len() as f64,
            values: self.values.iter().enumerate(),
        }
    }
}

pub struct SpectrumSegments<'a> {
    start: f64,
    segment_width: f64,
    values: Enumerate<Iter<'a, f64>>,
}

impl<'a> Iterator for SpectrumSegments<'a> {
    type Item = Segment;

    fn next(&mut self) -> Option<Segment> {
        match self.values.next() {
            Some((i, &v)) => Some(Segment {
                start: self.start + i as f64 * self.segment_width,
                width: self.segment_width,
                value: v,
            }),
            None => None,
        }
    }
}

pub struct Segment {
    pub start: f64,
    pub width: f64,
    pub value: f64,
}
