use std;
use std::cmp::min;
use std::cmp::Ordering::Equal;
use std::iter::Enumerate;
use std::slice::Iter;
use std::ops::Mul;

use num_cpus;

use rand;
use rand::{Rng, XorShiftRng};

use cgmath::{Vector, EuclideanVector, Vector2};

use config::Prelude;
use config::entry::{Entry, Object};

use tracer::{self, Bounce, RenderContext};
use cameras;
use world;

static DEFAULT_SPECTRUM_SAMPLES: u32 = 10;
static DEFAULT_SPECTRUM_BINS: usize = 64;
static DEFAULT_SPECTRUM_SPAN: (f64, f64) = (400.0, 700.0);

pub fn register_types(context: &mut Prelude) {
    context.object("Renderer".into()).object("Simple".into()).add_decoder(decode_simple);
}

pub struct Renderer {
    pub threads: usize,
    bounces: u32,
    pixel_samples: u32,
    light_samples: usize,
    spectrum_samples: u32,
    spectrum_bins: usize,
    spectrum_span: (f64, f64),
    algorithm: RenderAlgorithm
}

impl Renderer {
    pub fn make_tiles(&self, camera: &cameras::Camera, image_size: &Vector2<u32>) -> Vec<Tile> {
        self.algorithm.make_tiles(camera, image_size, self.spectrum_bins, self.spectrum_span)
    }

    pub fn render_tile(&self, tile: &mut Tile, camera: &cameras::Camera, world: &world::World) {
        self.algorithm.render_tile(tile, camera, world, self)
    }
}

enum RenderAlgorithm {
    Simple {tile_size: u32}
}

impl RenderAlgorithm {
    pub fn make_tiles(&self, camera: &cameras::Camera, image_size: &Vector2<u32>, spectrum_bins: usize, (spectrum_min, spectrum_max): (f64, f64)) -> Vec<Tile> {
        match *self {
            RenderAlgorithm::Simple {tile_size, ..} => {
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
                    let a = Vector2::new(a.screen_area.from.x as f32, a.screen_area.from.y as f32);
                    let b = Vector2::new(b.screen_area.from.x as f32, b.screen_area.from.y as f32);
                    let half_size = Vector2::new(image_size.x as f32 / 2.0, image_size.y as f32 / 2.0);
                    a.sub_v(&half_size).length2().partial_cmp(&b.sub_v(&half_size).length2()).unwrap_or(Equal)
                });
                tiles
            }
        }
    }

    pub fn render_tile(&self, tile: &mut Tile, camera: &cameras::Camera, world: &world::World, renderer: &Renderer) {
        match *self {
            RenderAlgorithm::Simple {..} => {
                let mut rng: XorShiftRng = rand::thread_rng().gen();

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

                    let mut used_additional = false;
                    let mut additional_samples: Vec<_> = (0..renderer.spectrum_samples-1).map(|_| (Sample {
                        wavelength: tile.sample_wavelength(&mut rng),
                        brightness: 0.0,
                        weight: 1.0,
                    }, 1.0)).collect();

                    for bounce in &path {
                        for &mut (ref mut sample, ref mut reflectance) in &mut additional_samples {
                            used_additional = contribute(bounce, sample, reflectance, true) || used_additional;
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

    let mut light_added = false;

    let c = color.get(&context) * probability;

    if let tracer::BounceType::Emission = *ty {
        sample.brightness += c * *reflectance;
        light_added = true;
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

            light_added = true;
        }

        *reflectance *= ty.brdf(&incident, &normal.direction);
    }

    light_added
}

fn decode_renderer(items: Object, algorithm: RenderAlgorithm) -> Result<Renderer, String> {
    let threads = match items.get("threads") {
        Some(v) => try!(v.decode(), "threads"),
        None => num_cpus::get()
    };

    let bounces = match items.get("bounces") {
        Some(v) => try!(v.decode(), "bounces"),
        None => 8
    };

    let pixel_samples = match items.get("pixel_samples") {
        Some(v) => try!(v.decode(), "pixel_samples"),
        None => 10
    };

    let light_samples = match items.get("light_samples") {
        Some(v) => try!(v.decode(), "light_samples"),
        None => 4
    };

    let (spectrum_samples, spectrum_bins, spectrum_span) = match items.get("spectrum").map(|e| e.as_object()) {
        Some(Some(v)) => try!(decode_spectrum(v), "spectrum"),
        Some(None) => return Err(format!("spectrum: expected a structure, but found something else")), //TODO: Print what we found
        None => (DEFAULT_SPECTRUM_SAMPLES, DEFAULT_SPECTRUM_BINS, DEFAULT_SPECTRUM_SPAN)
    };

    Ok(
        Renderer {
            threads: threads,
            bounces: bounces,
            pixel_samples: pixel_samples,
            light_samples: light_samples,
            spectrum_samples: spectrum_samples,
            spectrum_bins: spectrum_bins,
            spectrum_span: spectrum_span,
            algorithm: algorithm
        }
    )
}

fn decode_spectrum(items: Object) -> Result<(u32, usize, (f64, f64)), String> {
    let samples = match items.get("samples") {
        Some(v) => try!(v.decode(), "samples"),
        None => DEFAULT_SPECTRUM_SAMPLES
    };

    let bins = match items.get("bins") {
        Some(v) => try!(v.decode(), "bins"),
        None => DEFAULT_SPECTRUM_BINS
    };

    let span = match items.get("span") {
        Some(v) => try!(v.decode(), "span"),
        None => DEFAULT_SPECTRUM_SPAN
    };

    Ok((samples, bins, span))
}

fn decode_simple(entry: Entry) -> Result<Renderer, String> {
    let items = try!(entry.as_object().ok_or("not an object".into()));

    let tile_size = match items.get("tile_size") {
        Some(v) => try!(v.decode(), "tile_size"),
        None => 64
    };

    let algorithm = RenderAlgorithm::Simple {
        tile_size: tile_size,
    };

    decode_renderer(items, algorithm)
}



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

impl<'a> std::iter::Iterator for SpectrumSegments<'a> {
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