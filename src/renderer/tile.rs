use std::cmp::{min, PartialOrd};
use std::ops::{Mul, Sub};
use std::slice::Iter;
use std::iter::Enumerate;
use std::collections::hash_map::{self, HashMap, Entry};

use rand::Rng;

use cgmath::{Vector, Vector2};

pub struct Spectrum {
    pub min: f64,
    pub width: f64,
    values: Vec<f64>
}

impl Spectrum {
    pub fn from_pixel(pixel: &Pixel, from: f64, to: f64) -> Spectrum {
        Spectrum {
            min: from,
            width: to - from,
            values: pixel.final_values()
        }
    }

    pub fn segments(&self) -> SpectrumSegments {
        SpectrumSegments {
            start: self.min,
            segment_width: self.width / self.values.len() as f64,
            values: self.values.iter().enumerate()
        }
    }
}

pub struct SpectrumSegments<'a> {
    start: f64,
    segment_width: f64,
    values: Enumerate<Iter<'a, f64>>
}

impl<'a> Iterator for SpectrumSegments<'a> {
    type Item = Segment;

    fn next(&mut self) -> Option<Segment> {
        match self.values.next() {
            Some((i, &v)) => Some(Segment {
                start: self.start + i as f64 * self.segment_width,
                width: self.segment_width,
                value: v
            }),
            None => None
        }
    }
}

pub struct Segment {
    pub start: f64,
    pub width: f64,
    pub value: f64
}

pub struct Area<S> {
    pub from: Vector2<S>,
    pub size: Vector2<S>
}

impl<S> Area<S> {
    pub fn new(from: Vector2<S>, size: Vector2<S>) -> Area<S> {
        Area {
            from: from,
            size: size
        }
    }

    pub fn area(&self) -> S where for<'a> &'a S: Mul<&'a S, Output=S> {
        &self.size.x * &self.size.y
    }

    pub fn contains(&self, point: &Vector2<S>) -> bool where
        for<'a> &'a S: PartialOrd<&'a S>,
        for<'a> &'a S: Sub<&'a S, Output=S>
    {
        &self.from.x <= &point.x && &self.size.x > &(&point.x - &self.from.x) &&
        &self.from.y <= &point.y && &self.size.y > &(&point.y - &self.from.y)
    }
}

#[derive(Clone)]
pub struct Pixel {
    spectrum: Vec<(f64, f64)>
}

impl Pixel {
    pub fn new(steps: usize) -> Pixel {
        Pixel {
            spectrum: vec![(0.0, 0.0); steps]
        }
    }

    pub fn final_values(&self) -> Vec<f64> {
        self.spectrum.iter().map(|&(b, w)| if w > 0.0 { b / w } else { 0.0 }).collect()
    }

    pub fn merge(&mut self, other: &Pixel) {
        for (&mut(ref mut self_sum, ref mut self_weight), &(other_sum, other_weight)) in &mut self.spectrum.iter_mut().zip(&other.spectrum) {
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

pub struct Pixels<'a> {
    tile: &'a Tile,
    x_counter: u32,
    y_counter: u32
}

impl<'a> Iterator for Pixels<'a> {
    type Item = (&'a Pixel, Vector2<u32>);

    fn next(&mut self) -> Option<(&'a Pixel, Vector2<u32>)> {
        if self.y_counter >= self.tile.screen_area().size.y {
            None
        } else {
            let position = self.tile.screen_area().from.add_v(&Vector2::new(self.x_counter, self.y_counter));
            let pixel = self.tile.pixel(self.x_counter, self.y_counter);

            self.x_counter += 1;
            if self.x_counter >= self.tile.screen_area().size.x {
                self.x_counter = 0;
                self.y_counter += 1;
            }

            Some((pixel, position))
        }
    }
}

#[derive(Clone)]
pub struct Sample {
    pub brightness: f64,
    pub wavelength: f64,
    pub weight: f64
}

pub struct Tile {
    screen_area: Area<u32>,
    camera_area: Area<f64>,
    wavelength_from: f64,
    wavelength_to: f64,
    wavelength_width: f64,
    spectrum_steps: usize,
    screen_camera_ratio: Vector2<f64>,
    pixels: Vec<Pixel>,
    bonus_samples: LimitedMap,
}

impl Tile {
    pub fn new(screen_area: Area<u32>, camera_area: Area<f64>, wavelength_from: f64, wavelength_to: f64, spectrum_steps: usize) -> Tile {
        let area = screen_area.area();
        let screen_w = screen_area.size.x;
        let screen_h = screen_area.size.y;
        let camera_w = camera_area.size.x;
        let camera_h = camera_area.size.y;
        Tile {
            screen_area: screen_area,
            camera_area: camera_area,
            wavelength_from: wavelength_from,
            wavelength_to: wavelength_to,
            wavelength_width: wavelength_to - wavelength_from,
            spectrum_steps: spectrum_steps,
            screen_camera_ratio: Vector2::new(screen_w as f64 / camera_w, screen_h as f64 / camera_h),
            pixels: (0..area).map(|_| Pixel::new(spectrum_steps)).collect(),
            bonus_samples: LimitedMap::new(area as usize * 4, spectrum_steps),
        }
    }

    pub fn screen_area(&self) -> &Area<u32> {
        &self.screen_area
    }

    pub fn pixel_count(&self) -> usize {
        self.pixels.len()
    }

    pub fn pixels(&self) -> Pixels {
        Pixels {
            tile: self,
            x_counter: 0,
            y_counter: 0
        }
    }

    pub fn pixel(&self, x: u32, y: u32) -> &Pixel {
        &self.pixels[x as usize + y as usize * self.screen_area.size.x as usize]
    }

    pub fn sample_position<R: Rng>(&self, rng: &mut R) -> Vector2<f64> {
        let x = rng.gen_range(0.0, self.camera_area.size.x);
        let y = rng.gen_range(0.0, self.camera_area.size.y);
        self.camera_area.from.add_v(&Vector2::new(x, y))
    }

    pub fn sample_wavelength<R: Rng>(&self, rng: &mut R) -> f64 {
        rng.gen_range(self.wavelength_from, self.wavelength_to)
    }

    pub fn expose(&mut self, sample: Sample, position: Vector2<f64>) {
        if self.camera_area.contains(&position) {
            let offset = position.sub_v(&self.camera_area.from);
            let x = (offset.x * self.screen_camera_ratio.x) as usize;
            let y = (offset.y * self.screen_camera_ratio.y) as usize;

            let index = ((sample.wavelength - self.wavelength_from) / self.wavelength_width * self.spectrum_steps as f64) as usize;
            self.pixels[x + y * self.screen_area.size.x as usize].add_sample(PixelSample {
                value: sample.brightness,
                weight: sample.weight,
                bin: min(index, index - 1)
            });
        } else if 
            position.x > -1.0 && position.x < 1.0 &&
            position.y > -1.0 && position.y < 1.0
        {

            let index = ((sample.wavelength - self.wavelength_from) / self.wavelength_width * self.spectrum_steps as f64) as usize;

            let x = (position.x * self.screen_camera_ratio.x) as isize;
            let y = (position.y * self.screen_camera_ratio.y) as isize;
            self.bonus_samples.insert(Vector2::new(x, y), PixelSample {
                value: sample.brightness,
                weight: sample.weight,
                bin: min(index, index - 1)
            });
        }
    }

    pub fn bonus_samples(&self, image_size: &Vector2<u32>) -> BonusSamples {
        let size = Vector2::new(image_size.x as isize, image_size.y as isize);

        BonusSamples {
            samples: self.bonus_samples.iter(),
            image_size: size,
        }
    }
}

pub struct BonusSamples<'a> {
    samples: hash_map::Iter<'a, Vector2<isize>, Pixel>,
    image_size: Vector2<isize>,
}

impl<'a> Iterator for BonusSamples<'a> {
    type Item = (&'a Pixel, Vector2<usize>);

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if let Some((position, sample)) = self.samples.next() {
                let image_pos = position.add_v(&self.image_size.div_s(2));

                if image_pos.x >= 0 && image_pos.x < self.image_size.x && image_pos.y >= 0 && image_pos.y < self.image_size.y {
                    return Some((sample, Vector2::new(image_pos.x as usize, image_pos.y as usize)));
                }
            } else {
                return None
            }
        }
    }
}

pub struct PixelSample {
    value: f64,
    weight: f64,
    bin: usize
}

struct LimitedMap {
    limit: usize,
    bins: usize,
    map: HashMap<Vector2<isize>, Pixel>,
}

impl LimitedMap {
    fn new(limit: usize, bins: usize) -> LimitedMap {
        LimitedMap {
            limit: limit,
            bins: bins,
            map: HashMap::new()
        }
    }

    fn insert(&mut self, pos: Vector2<isize>, sample: PixelSample) {
        let len = self.map.len();
        match self.map.entry(pos) {
            Entry::Vacant(e) => if len < self.limit {
                let mut p = Pixel::new(self.bins);
                p.add_sample(sample);
                e.insert(p);
            },
            Entry::Occupied(mut e) => e.get_mut().add_sample(sample),
        }
    }

    fn iter(&self) -> hash_map::Iter<Vector2<isize>, Pixel> {
        self.map.iter()
    }
}
