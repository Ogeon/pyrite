use std::rand;
use std::rand::{Rng, XorShiftRng, Rand};
use std::ops::Mul;
use std::cmp::min;
use std::iter::Iterator;

use cgmath::vector::{Vector, Vector2};
use cgmath::ray::Ray3;

pub trait Camera {
    fn to_view_area(&self, area: &Area<uint>) -> Area<f64>;
    fn ray_towards(&self, target: &Vector2<f64>) -> Ray3<f64>;
}

pub trait World {
    fn intersect(&self, ray: &Ray3<f64>) -> Option<(Ray3<f64>, f64)>;
}

pub struct Spectrum {
    min: f64,
    width: f64,
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

    pub fn value_at(&self, frequency: f64) -> f64 {
        let index = ((frequency - self.min) / self.width * self.values.len() as f64) as uint;
        self.values[min(index, index - 1)]
    }
}

pub struct Area<S> {
    pub from: Vector2<S>,
    pub size: Vector2<S>
}

impl<S: Mul<S, S>> Area<S> {
    pub fn new(from: Vector2<S>, size: Vector2<S>) -> Area<S> {
        Area {
            from: from,
            size: size
        }
    }

    pub fn area(&self) -> S {
        self.size.x * self.size.y
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
    x_counter: uint,
    y_counter: uint
}

impl<'a> Iterator<(Spectrum, Vector2<uint>)> for Pixels<'a> {
    fn next(&mut self) -> Option<(Spectrum, Vector2<uint>)> {
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

struct Sample {
    pub brightness: f64,
    pub frequency: f64,
    pub weight: f64,
    pub position: Vector2<f64>
}

pub struct Tile {
    screen_area: Area<uint>,
    camera_area: Area<f64>,
    frequency_from: f64,
    frequency_to: f64,
    frequency_width: f64,
    screen_camera_ratio: Vector2<f64>,
    pixels: Vec<Pixel>
}

impl Tile {
    pub fn new(screen_area: Area<uint>, camera_area: Area<f64>, frequency_from: f64, frequency_to: f64, spectrum_steps: uint) -> Tile {
        Tile {
            screen_area: screen_area,
            camera_area: camera_area,
            frequency_from: frequency_from,
            frequency_to: frequency_to,
            frequency_width: frequency_to - frequency_from,
            screen_camera_ratio: Vector2::new(screen_area.size.x as f64 / camera_area.size.x, screen_area.size.y as f64 / camera_area.size.y),
            pixels: Vec::from_fn(screen_area.area(), |_| Pixel {
                spectrum: Vec::from_elem(spectrum_steps, (0.0, 0.0))
            })
        }
    }

    pub fn screen_area(&self) -> &Area<uint> {
        &self.screen_area
    }

    pub fn pixel_count(&self) -> uint {
        self.pixels.len()
    }

    pub fn pixels(&self) -> Pixels {
        Pixels {
            tile: self,
            x_counter: 0,
            y_counter: 0
        }
    }

    pub fn pixel(&self, x: uint, y: uint) -> Spectrum {
        let values = self.pixels[x + y * self.screen_area.size.x].final_values();
        Spectrum::new(self.frequency_from, self.frequency_to, values)
    }

    pub fn sample_position<R: Rng>(&self, rng: &mut R) -> Vector2<f64> {
        let x = rng.gen_range(0.0, self.camera_area.size.x);
        let y = rng.gen_range(0.0, self.camera_area.size.y);
        self.camera_area.from.add_v(&Vector2::new(x, y))
    }

    pub fn sample_frequency<R: Rng>(&self, rng: &mut R) -> f64 {
        rng.gen_range(self.frequency_from, self.frequency_to)
    }

    pub fn expose(&mut self, sample: Sample) {
        let offset = sample.position.sub_v(&self.camera_area.from);
        let x = (offset.x * self.screen_camera_ratio.x) as uint;
        let y = (offset.y * self.screen_camera_ratio.y) as uint;
        let &Pixel{spectrum: ref mut spectrum} = self.pixels.get_mut(x + y * self.screen_area.size.x);

        let index = ((sample.frequency - self.frequency_from) / self.frequency_width * spectrum.len() as f64) as uint;

        if index <= spectrum.len() {
            let (ref mut brightness, ref mut weight) = *spectrum.get_mut(min(index, index - 1));
            *brightness += sample.brightness * sample.weight;
            *weight += sample.weight;
        }
    }
}

struct RandomSequence<R> {
    rng: R,
    pos: uint,
    numbers: Vec<u32>
}

impl<R: Rng + Rand> RandomSequence<R> {
    pub fn new(rng: R) -> RandomSequence<R> {
        RandomSequence {
            rng: rng,
            pos: 0,
            numbers: Vec::new()
        }
    }

    pub fn mutate(&mut self) -> RandomSequence<R> {
        let mut rng: R = self.rng.gen();
        RandomSequence {
            pos: 0,
            numbers: self.numbers.iter().map(|&n| {
                if 0.5f32 < rng.gen() {
                    if rng.gen() {
                        n + rng.gen_range(0, 100_000_000)
                    } else {
                        n - rng.gen_range(0, 100_000_000)
                    }
                } else {
                    rng.gen()
                }
            }).collect(),
            rng: rng,
        }
    }

    pub fn generator(&mut self) -> &mut R {
        &mut self.rng
    }
}

impl<R: Rng> Rng for RandomSequence<R> {
    fn next_u32(&mut self) -> u32 {
        if self.numbers.len() == self.pos {
            self.numbers.push(self.rng.gen())
        }

        let v = self.numbers[self.pos];
        self.pos += 1;
        v
    }
}

pub fn render<C: Camera, W: World>(tile: &mut Tile, samples: uint, camera: &C, world: &W) {
    let mut rng: RandomSequence<XorShiftRng> = RandomSequence::new(rand::task_rng().gen());
    let mut old_sample = sample(tile, &mut rng, camera, world);
    old_sample.weight = 1.0;

    for _ in range(0, tile.pixel_count() * samples - 1) {
        let mut new_rng = rng.mutate();
        let mut new_sample = sample(tile, &mut new_rng, camera, world);

        let a = ((new_sample.brightness + 1.0) / (old_sample.brightness + 1.0)).min(1.0);

        new_sample.weight = a;
        old_sample.weight += (1.0 - a);

        if a >= new_rng.generator().gen() {
            rng = new_rng;
            tile.expose(old_sample);
            old_sample = new_sample;
        } else {
            tile.expose(new_sample);
        }

    }
}

fn sample<R: Rng, C: Camera, W: World>(tile: &Tile, rng: &mut R, camera: &C, world: &W) -> Sample {
    let position = tile.sample_position(rng);
    let frequency = tile.sample_frequency(rng);

    let ray = camera.ray_towards(&position);
    let brightness = match world.intersect(&ray) {
        Some((normal, distance)) => 1.0 / distance,
        None => 0.0
    };

    Sample {
        brightness: brightness,
        frequency: frequency,
        weight: 0.0,
        position: position
    }
}