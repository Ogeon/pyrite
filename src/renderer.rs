use std;
use std::collections::HashMap;
use std::cmp::min;
use std::rand;
use std::rand::{Rng, XorShiftRng};
use std::iter::Enumerate;
use std::slice::Items;

use cgmath::vector::{Vector, Vector2};

use config;
use config::FromConfig;

use cameras;

use tracer;

pub fn register_types(context: &mut config::ConfigContext) {
	context.insert_grouped_type("Renderer", "Simple", decode_simple);
}

pub struct Renderer {
    pub threads: uint,
    bounces: uint,
    pixel_samples: uint,
    spectrum_samples: uint,
    spectrum_bins: uint,
    algorithm: RenderAlgorithm
}

impl Renderer {
    pub fn make_tiles(&self, camera: &cameras::Camera, image_size: &Vector2<uint>) -> Vec<Tile> {
        self.algorithm.make_tiles(camera, image_size, self.spectrum_bins)
    }

    pub fn render_tile(&self, tile: &mut Tile, camera: &cameras::Camera, world: &tracer::World) {
        self.algorithm.render_tile(tile, camera, world, self)
    }
}

enum RenderAlgorithm {
	Simple {tile_size: uint}
}

impl RenderAlgorithm {
	pub fn make_tiles(&self, camera: &cameras::Camera, image_size: &Vector2<uint>, spectrum_bins: uint) -> Vec<Tile> {
		match *self {
			Simple {tile_size, ..} => {
				let tiles_x = (image_size.x as f32 / tile_size as f32).ceil() as uint;
			    let tiles_y = (image_size.y as f32 / tile_size as f32).ceil() as uint;

			    let mut tiles = Vec::new();

			    for y in range(0, tiles_y) {
			        for x in range(0, tiles_x) {
			            let from = Vector2::new(x * tile_size, y * tile_size);
			            let size = Vector2::new(min(image_size.x - from.x, tile_size), min(image_size.y - from.y, tile_size));

			            let image_area = Area::new(from, size);
			            let camera_area = camera.to_view_area(&image_area, image_size);

			            tiles.push(Tile::new(image_area, camera_area, 400.0, 700.0, spectrum_bins));
			        }
			    }

			    tiles
			}
		}
	}

	pub fn render_tile(&self, tile: &mut Tile, camera: &cameras::Camera, world: &tracer::World, renderer: &Renderer) {
		match *self {
			Simple {..} => {
				let mut rng: XorShiftRng = rand::task_rng().gen();

				for _ in range(0, tile.pixel_count() * renderer.pixel_samples) {
					let position = tile.sample_position(&mut rng);
					let wavelengths = range(0, renderer.spectrum_samples).map(|_| tile.sample_wavelength(&mut rng)).collect();

					let ray = camera.ray_towards(&position);
					let samples = tracer::trace(&mut rng, ray, wavelengths, world, renderer.bounces);

					for sample in samples.move_iter() {
						let sample = Sample {
							brightness: sample.brightness,
							wavelength: sample.wavelength,
							weight: 1.0
						};
						tile.expose(sample, position);
					}
				}
			}
		}
	}
}

fn decode_renderer(_context: &config::ConfigContext, items: HashMap<String, config::ConfigItem>, algorithm: RenderAlgorithm) -> Result<Renderer, String> {
    let mut items = items;

    let threads = match items.pop_equiv(&"threads") {
        Some(v) => try!(FromConfig::from_config(v), "threads"),
        None => std::rt::default_sched_threads()
    };

    let bounces = match items.pop_equiv(&"bounces") {
        Some(v) => try!(FromConfig::from_config(v), "bounces"),
        None => 8
    };

    let pixel_samples = match items.pop_equiv(&"pixel_samples") {
        Some(v) => try!(FromConfig::from_config(v), "pixel_samples"),
        None => 10
    };

    let spectrum_samples = match items.pop_equiv(&"spectrum_samples") {
        Some(v) => try!(FromConfig::from_config(v), "spectrum_samples"),
        None => 5
    };

    let spectrum_bins = match items.pop_equiv(&"spectrum_bins") {
        Some(v) => try!(FromConfig::from_config(v), "spectrum_bins"),
        None => 64
    };

    Ok(
        Renderer {
            threads: threads,
            bounces: bounces,
            pixel_samples: pixel_samples,
            spectrum_samples: spectrum_samples,
            spectrum_bins: spectrum_bins,
            algorithm: algorithm
        }
    )
}

fn decode_simple(context: &config::ConfigContext, items: HashMap<String, config::ConfigItem>) -> Result<Renderer, String> {
    let mut items = items;

	let tile_size = match items.pop_equiv(&"tile_size") {
		Some(v) => try!(FromConfig::from_config(v), "tile_size"),
		None => 64
	};

    let algorithm = Simple {
        tile_size: tile_size,
    };

    decode_renderer(context, items, algorithm)
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

    pub fn value_at(&self, wavelength: f64) -> f64 {
    	if wavelength < self.min || wavelength > self.min + self.width {
    		0.0
    	} else {
    		let index = ((wavelength - self.min) / self.width * self.values.len() as f64) as uint;
    		self.values[min(index, index - 1)]
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
    values: Enumerate<Items<'a, f64>>
}

impl<'a> std::iter::Iterator<Segment> for SpectrumSegments<'a> {
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

pub struct Sample {
    pub brightness: f64,
    pub wavelength: f64,
    pub weight: f64
}

pub struct Tile {
    screen_area: Area<uint>,
    camera_area: Area<f64>,
    wavelength_from: f64,
    wavelength_to: f64,
    wavelength_width: f64,
    screen_camera_ratio: Vector2<f64>,
    pixels: Vec<Pixel>
}

impl Tile {
    pub fn new(screen_area: Area<uint>, camera_area: Area<f64>, wavelength_from: f64, wavelength_to: f64, spectrum_steps: uint) -> Tile {
        Tile {
            screen_area: screen_area,
            camera_area: camera_area,
            wavelength_from: wavelength_from,
            wavelength_to: wavelength_to,
            wavelength_width: wavelength_to - wavelength_from,
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
        let x = (offset.x * self.screen_camera_ratio.x) as uint;
        let y = (offset.y * self.screen_camera_ratio.y) as uint;
        let &Pixel{spectrum: ref mut spectrum} = self.pixels.get_mut(x + y * self.screen_area.size.x);

        let index = ((sample.wavelength - self.wavelength_from) / self.wavelength_width * spectrum.len() as f64) as uint;

        if index <= spectrum.len() {
            let (ref mut brightness, ref mut weight) = *spectrum.get_mut(min(index, index - 1));
            *brightness += sample.brightness * sample.weight;
            *weight += sample.weight;
        }
    }
}