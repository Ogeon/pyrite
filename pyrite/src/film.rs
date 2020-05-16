use crossbeam::atomic::AtomicCell;

use noisy_float::prelude::*;

use cgmath::{BaseNum, Point2, Vector2};

use rand::Rng;
use std::{iter::Enumerate, slice::Iter};

pub struct Film {
    width: usize,
    height: usize,
    aspect_ratio: AspectRatio,
    grains_per_pixel: usize,
    wavelength_start: f64,
    wavelength_width: f64,
    grains_per_wavelength: f64,
    grains: Vec<Grain>,
}

impl Film {
    pub fn new(
        width: usize,
        height: usize,
        grains_per_pixel: usize,
        wavelength_span: (f64, f64),
    ) -> Self {
        let length = width * height * grains_per_pixel;
        let (wavelength_start, wavelength_end) = wavelength_span;
        let wavelength_width = wavelength_end - wavelength_start;

        Self {
            width,
            height,
            aspect_ratio: AspectRatio::new(width, height),
            grains_per_pixel,
            wavelength_start,
            wavelength_width,
            grains_per_wavelength: grains_per_pixel as f64 / wavelength_width,
            grains: std::iter::repeat_with(Grain::new).take(length).collect(),
        }
    }

    pub fn width(&self) -> usize {
        self.width
    }

    pub fn height(&self) -> usize {
        self.height
    }

    pub fn get_pixel(&self, position: Point2<usize>) -> Option<&[Grain]> {
        if position.x >= self.width || position.y >= self.height {
            return None;
        }

        let index = (position.x + position.y * self.width) * self.grains_per_pixel;
        Some(&self.grains[index..index + self.grains_per_pixel])
    }

    pub fn get_pixel_f(&self, position: Point2<f64>) -> Option<&[Grain]> {
        self.get_pixel(self.aspect_ratio.to_pixel(position)?)
    }

    pub fn sample_wavelength<R: Rng>(&self, rng: &mut R) -> f64 {
        rng.gen_range(
            self.wavelength_start,
            self.wavelength_start + self.wavelength_width,
        )
    }

    fn wavelength_to_grain(&self, wavelength: f64) -> usize {
        ((wavelength - self.wavelength_start) * self.grains_per_wavelength) as usize
    }

    pub fn expose(&self, position: Point2<f64>, sample: Sample) {
        let grain_index = self.wavelength_to_grain(sample.wavelength);

        if let Some(pixel) = self.get_pixel_f(position) {
            pixel[grain_index].expose(sample.brightness, sample.weight);
        }
    }

    pub fn get_pixel_ref_f(&self, position: Point2<f64>) -> Option<DetachedPixel> {
        Some(DetachedPixel {
            grains: self.get_pixel_f(position)?,
        })
    }

    pub fn to_pixel_sample(&self, sample: &Sample) -> PixelSample {
        PixelSample {
            value: sample.brightness,
            weight: sample.weight,
            grain: self.wavelength_to_grain(sample.wavelength),
        }
    }

    pub fn developed_pixels(&self) -> DevelopedPixels<'_> {
        DevelopedPixels::new(self)
    }
}

pub struct Grain {
    weight: AtomicCell<N64>,
    accumulator: AtomicCell<N64>,
}

impl Grain {
    fn new() -> Self {
        Self {
            weight: AtomicCell::new(n64(0.0)),
            accumulator: AtomicCell::new(n64(0.0)),
        }
    }

    pub fn expose(&self, value: f64, weight: f64) {
        self.increment(value * weight, weight);
    }

    pub fn develop(&self) -> f64 {
        let weight = self.weight.load();
        let accumulator = self.accumulator.load();

        if weight > 0.0 {
            (accumulator / weight).into()
        } else {
            0.0
        }
    }

    fn increment(&self, increment: f64, weight: f64) {
        let mut current_weight = self.weight.load();
        loop {
            let result = self
                .weight
                .compare_exchange(current_weight, current_weight + weight);

            if let Err(current) = result {
                current_weight = current;
            } else {
                break;
            }
        }

        let mut current_accumulator = self.accumulator.load();
        loop {
            let result = self
                .accumulator
                .compare_exchange(current_accumulator, current_accumulator + increment);

            if let Err(current) = result {
                current_accumulator = current;
            } else {
                break;
            }
        }
    }
}

pub struct DetachedPixel<'a> {
    grains: &'a [Grain],
}

impl<'a> DetachedPixel<'a> {
    pub fn expose(&self, sample: PixelSample) {
        self.grains[sample.grain].expose(sample.value, sample.weight);
    }
}

pub struct PixelSample {
    value: f64,
    weight: f64,
    grain: usize,
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

    fn contains(&self, point: Point2<f64>) -> bool {
        match self.orientation {
            Orientation::Horizontal => point.y.abs() <= self.ratio,
            Orientation::Vertical => point.x.abs() <= self.ratio,
        }
    }

    fn to_pixel(&self, point: Point2<f64>) -> Option<Point2<usize>> {
        if self.contains(point) {
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
}

#[derive(Clone)]
pub struct Sample {
    pub brightness: f64,
    pub wavelength: f64,
    pub weight: f64,
}

pub struct DevelopedPixels<'a> {
    index: usize,
    film: &'a Film,
}

impl<'a> DevelopedPixels<'a> {
    fn new(film: &'a Film) -> Self {
        Self { index: 0, film }
    }
}

impl<'a> Iterator for DevelopedPixels<'a> {
    type Item = Spectrum<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        let end = self.index + self.film.grains_per_pixel;

        let result = if end < self.film.grains.len() {
            Some(Spectrum {
                min: self.film.wavelength_start,
                width: self.film.wavelength_width,
                grains: &self.film.grains[self.index..end],
            })
        } else {
            None
        };

        self.index = end;

        result
    }
}

pub struct Spectrum<'a> {
    pub min: f64,
    pub width: f64,
    grains: &'a [Grain],
}

impl<'a> Spectrum<'a> {
    pub fn segments(&self) -> SpectrumSegments<'_> {
        SpectrumSegments {
            start: self.min,
            segment_width: self.width / self.grains.len() as f64,
            grains: self.grains.iter().enumerate(),
        }
    }
}

pub struct SpectrumSegments<'a> {
    start: f64,
    segment_width: f64,
    grains: Enumerate<Iter<'a, Grain>>,
}

impl<'a> Iterator for SpectrumSegments<'a> {
    type Item = Segment;

    fn next(&mut self) -> Option<Segment> {
        match self.grains.next() {
            Some((i, v)) => Some(Segment {
                start: self.start + i as f64 * self.segment_width,
                width: self.segment_width,
                value: v.develop(),
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
