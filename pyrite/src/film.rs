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
        loop {
            let result = self
                .data
                .compare_exchange(currant_data, currant_data.add(increment, weight));

            if let Err(current) = result {
                currant_data = current;
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
    min: f32,
    width: f32,
    grains: &'a [Grain],
}

impl<'a> Spectrum<'a> {
    pub fn segments(&self) -> SpectrumSegments<'_> {
        SpectrumSegments {
            start: self.min,
            segment_width: self.width / self.grains.len() as f32,
            grains: self.grains.iter().enumerate(),
        }
    }

    pub fn segments_between(&self, min: f32, max: f32, segments: usize) -> SegmentsBetween<'_> {
        SegmentsBetween::new(self.segments(), min, max, segments)
    }

    pub fn spectrum_width(&self) -> (f32, f32) {
        (self.min, self.min + self.width)
    }
}

pub struct SpectrumSegments<'a> {
    start: f32,
    segment_width: f32,
    grains: Enumerate<Iter<'a, Grain>>,
}

impl<'a> Iterator for SpectrumSegments<'a> {
    type Item = Segment<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        match self.grains.next() {
            Some((i, v)) => Some(Segment {
                start: self.start + i as f32 * self.segment_width,
                end: self.start + (i + 1) as f32 * self.segment_width,
                grain: v,
            }),
            None => None,
        }
    }
}

pub struct SegmentsBetween<'a> {
    from: f32,
    segment_size: f32,
    segments: usize,
    current_segment: usize,
    start_grain: Option<Segment<'a>>,
    end_grain: Option<Segment<'a>>,
    grains: std::iter::Peekable<SpectrumSegments<'a>>,
}

impl<'a> SegmentsBetween<'a> {
    fn new(grains: SpectrumSegments<'a>, min: f32, max: f32, segments: usize) -> Self {
        if segments < 1 {
            panic!("need at least one segment");
        }
        let mut grains = grains.peekable();

        let segment_size = (max - min) / segments as f32;

        let start = min;
        let end = min + segment_size;
        let mut start_grain = None;
        let mut end_grain = None;

        while let Some(&grain) = grains.peek() {
            let min = grain.start;
            let max = grain.end;

            if min > end {
                break;
            }

            grains.next();

            if min <= start && max > start {
                start_grain = Some(grain);
            }

            if min <= end && max > end {
                end_grain = Some(grain);
            }
        }

        SegmentsBetween {
            from: min,
            segment_size,
            segments,
            current_segment: 0,
            start_grain,
            end_grain,
            grains,
        }
    }
}

impl<'a> Iterator for SegmentsBetween<'a> {
    type Item = ((f32, f32), (f32, f32));

    fn next(&mut self) -> Option<Self::Item> {
        if self.current_segment >= self.segments {
            return None;
        }

        let start = self.current_segment as f32 * self.segment_size + self.from;
        let end = (self.current_segment + 1) as f32 * self.segment_size + self.from;

        let result = (
            self.start_grain
                .map(|grain| (start, grain.value()))
                .unwrap_or((start, 0.0)),
            self.end_grain
                .map(|grain| (end, grain.value()))
                .unwrap_or((end, 0.0)),
        );

        self.current_segment += 1;
        self.start_grain = self.end_grain;

        let next_end = (self.current_segment + 1) as f32 * self.segment_size + self.from;

        let find_new_end = if let Some(grain) = self.end_grain {
            grain.end <= next_end
        } else {
            true
        };

        if find_new_end {
            let mut end_grain = None;

            while let Some(&grain) = self.grains.peek() {
                let min = grain.start;
                let max = grain.end;

                if min > next_end {
                    break;
                }

                self.grains.next();

                if min <= next_end && max > next_end {
                    end_grain = Some(grain);
                }
            }

            self.end_grain = end_grain;
        }

        Some(result)
    }
}

#[derive(Copy, Clone)]
pub struct Segment<'a> {
    pub start: f32,
    pub end: f32,
    grain: &'a Grain,
}

impl<'a> Segment<'a> {
    pub fn value(&self) -> f32 {
        self.grain.develop()
    }
}
