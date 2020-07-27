use crossbeam::atomic::AtomicCell;

use noisy_float::prelude::*;

use cgmath::{BaseNum, Point2, Vector2};

use rand::Rng;

pub struct Film {
    width: usize,
    height: usize,
    aspect_ratio: AspectRatio,
    grains_per_pixel: usize,
    wavelength_start: f32,
    wavelength_width: f32,
    grains_per_wavelength: f32,
    grains: Vec<Grain>,
}

impl Film {
    pub fn new(
        width: usize,
        height: usize,
        grains_per_pixel: usize,
        wavelength_span: (f32, f32),
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
            grains_per_wavelength: grains_per_pixel as f32 / wavelength_width,
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

    pub fn get_pixel_f(&self, position: Point2<f32>) -> Option<&[Grain]> {
        self.get_pixel(self.aspect_ratio.to_pixel(position)?)
    }

    pub fn sample_wavelength<R: Rng>(&self, rng: &mut R) -> f32 {
        rng.gen_range(
            self.wavelength_start,
            self.wavelength_start + self.wavelength_width,
        )
    }

    fn wavelength_to_grain(&self, wavelength: f32) -> usize {
        ((wavelength - self.wavelength_start) * self.grains_per_wavelength) as usize
    }

    pub fn expose(&self, position: Point2<f32>, sample: Sample) {
        let grain_index = self.wavelength_to_grain(sample.wavelength);

        if let Some(pixel) = self.get_pixel_f(position) {
            pixel[grain_index].expose(sample.brightness, sample.weight);
        }
    }

    pub fn get_pixel_ref_f(&self, position: Point2<f32>) -> Option<DetachedPixel> {
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

#[repr(transparent)]
pub struct Grain {
    data: AtomicCell<GrainData>,
}

impl Grain {
    fn new() -> Self {
        Self {
            data: AtomicCell::new(GrainData::new()),
        }
    }

    pub fn expose(&self, value: f32, weight: f32) {
        self.increment(value * weight, weight);
    }

    pub fn develop(&self) -> f32 {
        let GrainData {
            weight,
            accumulator,
        } = self.data.load();

        if weight > 0.0 {
            (accumulator / weight).into()
        } else {
            0.0
        }
    }

    fn increment(&self, increment: f32, weight: f32) {
        let mut currant_data = self.data.load();
        let mut attempts = 0;

        // Discard the sample if multiple threads are stuck updating the grain
        while attempts < 5 {
            let result = self
                .data
                .compare_exchange(currant_data, currant_data.add(increment, weight));

            if let Err(current) = result {
                currant_data = current;
                attempts += 1;
            } else {
                break;
            }
        }
    }
}

#[derive(Copy, Clone, Eq, PartialEq)]
struct GrainData {
    accumulator: N32,
    weight: N32,
}

impl GrainData {
    fn new() -> Self {
        Self {
            weight: n32(0.0),
            accumulator: n32(0.0),
        }
    }

    fn add(&self, increment: f32, weight: f32) -> Self {
        Self {
            accumulator: self.accumulator + increment,
            weight: self.weight + weight,
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
    value: f32,
    weight: f32,
    grain: usize,
}

struct AspectRatio {
    size: f32,
    ratio: f32,
    orientation: Orientation,
}

impl AspectRatio {
    fn new(width: usize, height: usize) -> AspectRatio {
        if width >= height {
            AspectRatio {
                size: width as f32,
                ratio: height as f32 / width as f32,
                orientation: Orientation::Horizontal,
            }
        } else {
            AspectRatio {
                size: height as f32,
                ratio: width as f32 / height as f32,
                orientation: Orientation::Vertical,
            }
        }
    }

    fn contains(&self, point: Point2<f32>) -> bool {
        match self.orientation {
            Orientation::Horizontal => point.y.abs() <= self.ratio,
            Orientation::Vertical => point.x.abs() <= self.ratio,
        }
    }

    fn to_pixel(&self, point: Point2<f32>) -> Option<Point2<usize>> {
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
    pub brightness: f32,
    pub wavelength: f32,
    pub weight: f32,
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
                max: self.film.wavelength_start + self.film.wavelength_width,
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
    min: f32,
    max: f32,
    grains: &'a [Grain],
}

impl<'a> Spectrum<'a> {
    pub(crate) fn get(&self, wavelength: f32) -> f32 {
        let min = self.min;
        let max = self.max;

        match wavelength {
            w if w < min => 0.0,
            w if w > max => 0.0,
            w => {
                let normalized = (w - min) / (max - min);
                let float_index = normalized * self.grains.len() as f32;
                let index = (float_index.floor() as usize).min(self.grains.len() - 1);

                self.grains[index].develop()
            }
        }
    }

    pub fn spectrum_width(&self) -> (f32, f32) {
        (self.min, self.max)
    }
}
