use std::cmp::min;
use std::ops::Mul;
use std::slice::Iter;
use std::iter::Enumerate;

use rand::Rng;

use cgmath::{Vector, Vector2};

pub struct Spectrum {
    pub min: f64,
    pub width: f64,
    values: Vec<f64>
}

impl Spectrum {
    fn new(from: f64, to: f64, values: Vec<f64>) -> Spectrum {
        Spectrum {
            min: from,
            width: to - from,
            values: values
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

impl<S> Area<S> where for<'a> &'a S: Mul<&'a S, Output=S> {
    pub fn new(from: Vector2<S>, size: Vector2<S>) -> Area<S> {
        Area {
            from: from,
            size: size
        }
    }

    pub fn area(&self) -> S {
        &self.size.x * &self.size.y
    }
}

struct Pixel {
    pub spectrum: Vec<(f64, f64)>
}

impl Pixel {
    fn final_values(&self) -> Vec<f64> {
        self.spectrum.iter().map(|&(b, w)| if w > 0.0 { b / w } else { 0.0 }).collect()
    }
}

pub struct Pixels<'a> {
    tile: &'a Tile,
    x_counter: u32,
    y_counter: u32
}

impl<'a> Iterator for Pixels<'a> {
    type Item = (Spectrum, Vector2<u32>);

    fn next(&mut self) -> Option<(Spectrum, Vector2<u32>)> {
        if self.y_counter >= self.tile.screen_area().size.y {
            None
        } else {
            let position = self.tile.screen_area().from.add_v(&Vector2::new(self.x_counter, self.y_counter));
            let spectrum = self.tile.pixel(self.x_counter, self.y_counter);

            self.x_counter += 1;
            if self.x_counter >= self.tile.screen_area().size.x {
                self.x_counter = 0;
                self.y_counter += 1;
            }

            Some((spectrum, position))
        }
    }
}

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
    screen_camera_ratio: Vector2<f64>,
    pixels: Vec<Pixel>
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
            screen_camera_ratio: Vector2::new(screen_w as f64 / camera_w, screen_h as f64 / camera_h),
            pixels: (0..area).map(|_| Pixel {
                spectrum: vec![(0.0, 0.0); spectrum_steps]
            }).collect()
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

    pub fn pixel(&self, x: u32, y: u32) -> Spectrum {
        let values = self.pixels[x as usize + y as usize * self.screen_area.size.x as usize].final_values();
        Spectrum::new(self.wavelength_from, self.wavelength_to, values)
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
        let offset = position.sub_v(&self.camera_area.from);
        let x = (offset.x * self.screen_camera_ratio.x) as usize;
        let y = (offset.y * self.screen_camera_ratio.y) as usize;
        let &mut Pixel{ref mut spectrum} = &mut self.pixels[x + y * self.screen_area.size.x as usize];

        let index = ((sample.wavelength - self.wavelength_from) / self.wavelength_width * spectrum.len() as f64) as usize;

        if index <= spectrum.len() {
            let &mut (ref mut brightness, ref mut weight) = &mut spectrum[min(index, index - 1)];
            *brightness += sample.brightness * sample.weight;
            *weight += sample.weight;
        }
    }
}