use std;
use std::collections::HashMap;
use std::cmp::min;
use std::rand;
use std::rand::{Rng, XorShiftRng};

use cgmath::vector::{Vector, Vector2};

use config;
use config::FromConfig;

use cameras;

use tracer;

pub fn register_types(context: &mut config::ConfigContext) {
	context.insert_grouped_type("Renderer", "Simple", decode_simple);
}

pub enum Renderer {
	Simple {threads: uint, tile_size: uint, bounces: uint, samples: uint}
}

impl Renderer {
	pub fn threads(&self) -> uint {
		match *self {
			Simple {threads, ..} => threads,
		}
	}

	pub fn make_tiles(&self, camera: &cameras::Camera, image_size: &Vector2<uint>) -> Vec<Tile> {
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

			            tiles.push(Tile::new(image_area, camera_area, 300.0, 900.0, 3));
			        }
			    }

			    tiles
			}
		}
	}

	pub fn render_tile(&self, tile: &mut Tile, camera: &cameras::Camera, world: &tracer::World) {
		match *self {
			Simple {bounces, samples, ..} => {
				let mut rng: XorShiftRng = rand::task_rng().gen();

				for _ in range(0, tile.pixel_count() * samples) {
					let position = tile.sample_position(&mut rng);
					let frequency = tile.sample_frequency(&mut rng);

					let ray = camera.ray_towards(&position);
					let sample = Sample {
						brightness: tracer::trace(&mut rng, ray, frequency, world, bounces),
						frequency: frequency,
						weight: 1.0
					};

					tile.expose(sample, position);
				}
			}
		}
	}
}

fn decode_simple(_context: &config::ConfigContext, items: HashMap<String, config::ConfigItem>) -> Result<Renderer, String> {
	let mut items = items;

	let threads = match items.pop_equiv(&"threads") {
		Some(v) => try!(FromConfig::from_config(v), "threads"),
		None => std::rt::default_sched_threads()
	};

	let tile_size = match items.pop_equiv(&"tile_size") {
		Some(v) => try!(FromConfig::from_config(v), "tile_size"),
		None => 64
	};

	let bounces = match items.pop_equiv(&"bounces") {
		Some(v) => try!(FromConfig::from_config(v), "bounces"),
		None => 8
	};

	let samples = match items.pop_equiv(&"samples") {
		Some(v) => try!(FromConfig::from_config(v), "samples"),
		None => 10
	};

	Ok(Simple {
		threads: threads,
		tile_size: tile_size,
		bounces: bounces,
		samples: samples
	})
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
    	if frequency < self.min || frequency > self.min + self.width {
    		0.0
    	} else {
    		let index = ((frequency - self.min) / self.width * self.values.len() as f64) as uint;
    		self.values[min(index, index - 1)]
    	}
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

pub struct Sample {
    pub brightness: f64,
    pub frequency: f64,
    pub weight: f64
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

    pub fn expose(&mut self, sample: Sample, position: Vector2<f64>) {
        let offset = position.sub_v(&self.camera_area.from);
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