extern mod std;
use std::rand::{XorShiftRng, Rng};
use std::num::{exp, ln, min, max, sqrt};
use std::{task, fmt, vec};
use std::comm::{Chan, Data};
use std::iter::range;
use extra::arc::{MutexArc, Arc};
use nalgebra::na::{Vec3, Rot3, Rotate};
use nalgebra::na;

//Random variable

pub struct RandomVariable {
	values: ~[f32],
	pos: uint,
	random: XorShiftRng
}

impl RandomVariable {
	pub fn new(random: XorShiftRng) -> RandomVariable {
		RandomVariable{
			values: ~[],
			pos: 0,
			random: random
		}
	}

	pub fn next(&mut self) -> f32 {
		let pos = self.pos;
		self.pos += 1;

		while pos >= self.values.len() {
			self.values.push(self.random.gen());
		}

		self.values[pos]
	}

	pub fn rewind(&mut self) {
		self.pos = 0;
	}

	pub fn jump(&mut self, position: uint) {
		self.pos = position;
	}

	pub fn mutate<T: Rng>(&mut self, random: &mut T, ammount: f32) {
		let s1: f32 = 1.0 / 1024.0;
		let s2: f32 = 1.0 / 64.0;

		self.values = self.values.iter().map(|&v| {
			let mutation = ammount * s2 * exp(-ln(s2/s1) * random.gen());
			let mut new_value = if random.gen::<f32>() < 0.5 {v + mutation} else {v - mutation};

			while new_value < 0.0 {
				new_value += 1.0;
			}

			while new_value > 1.0 {
				new_value -= 1.0;
			}

			new_value
		}).collect();
	}

	pub fn clear(&mut self) {
		self.values = ~[];
		self.pos = 0;
	}

	pub fn set(&mut self, new_values: ~[f32]) {
		self.values = new_values;
		self.pos = 0;
	}
}

//Sample
struct Sample {
	frequency: f32,
	value: f32,
	weight: f32,
	pixel: (u32, u32)
}

impl fmt::Default for Sample {
	fn fmt(s: &Sample, f: &mut fmt::Formatter) {
		let (x, y) = s.pixel;
		write!(f.buf, "pixel: [{}, {}], freq: {} nm, value: {}, weight: {}",
					  x, y, s.frequency, s.value, s.weight)
	}
}

//Sampler
pub trait Sampler {
	fn random_variable<T: Rng>(random: &mut T) -> RandomVariable;
	fn sample(traced: Sample) -> Sample;
}

//Tracer
pub struct Tracer {
	samples: u32,
	active_tasks: ~MutexArc<~u16>,
	scene: ~Arc<Scene>,
	image_size: (u32, u32),
	tile_size: (u32, u32),
	tiles: ~MutexArc<~[Tile]>,
	bins: uint,
	pixels: ~MutexArc<~[~[f32]]>,
	freq_span: (f32, f32)
}

impl Tracer {
	pub fn new() -> Tracer {
		Tracer {
			samples: 10,
			active_tasks: ~MutexArc::new(~0),
			scene: ~Arc::new(Scene{
				camera: Camera::new(na::zero(), na::zero()),
				objects: ~[],
				materials: ~[]
			}),
			image_size: (512, 512),
			tile_size: (64, 64),
			tiles: ~MutexArc::new(~[]),
			bins: 3,
			pixels: ~MutexArc::new(vec::from_elem(512 * 512, vec::from_elem(3, 0.0f32))),
			freq_span: (400.0, 740.0)
		}
	}

	pub fn spawn(&mut self/*, sampler: S*/) -> TracerTask {
		if self.done() {
			let (image_w, image_h) = self.image_size;
			self.tiles = ~MutexArc::new(generate_tiles(self.image_size, self.tile_size));
			self.pixels = ~MutexArc::new(vec::from_elem((image_w * image_h) as uint, vec::from_elem(self.bins, 0.0f32)));
		}

		let (command_port, command_chan) = Chan::<TracerCommands>::new();
		let data = TracerData{
			command_port: command_port,
			tiles: self.tiles.clone(),//generate_tiles(self.image_size, self.tile_size),
			samples: self.samples,
			task_counter: self.active_tasks.clone(),
			scene: self.scene.clone(),
			bins: self.bins,
			image_size: self.image_size,
			pixels: self.pixels.clone(),
			freq_span: self.freq_span
		};

		let task_number = self.active_tasks.access(|&ref mut num| {
			**num += 1;
			**num-1
		});
		let mut new_task = task::task();
		new_task.name(format!("Task {}", task_number));
		new_task.spawn(proc(){
			Tracer::run(data);
		});

		TracerTask {
			command_chan: command_chan
		}
	}

	pub fn done(&self) -> bool {
		self.active_tasks.access(|&ref num| {**num == 0})
	}

	pub fn set_scene(&mut self, scene: Scene) {
		self.scene = ~Arc::new(scene);
	}

	fn run(data: TracerData) {
		task::deschedule();

		let mut running = true;
		let mut rand_var = RandomVariable::new(XorShiftRng::new());
		let (image_w, _) = data.image_size;
		let (min_freq, max_freq) = data.freq_span;
		let freq_diff = max_freq - min_freq;

		while running {
			let maybe_tile = data.tiles.access(Tracer::get_tile);

			match maybe_tile {
				Some(tile) => {
					let (pix_x, pix_y, pix_w, pix_h) = tile.screen;
					let (cam_x, cam_y, pixel_size) = tile.world;
					
					for x in range(0, pix_w) {
						let tile_column: ~[~[f32]] = range(0, pix_h).map(|y| {
							let mut values = vec::from_elem(data.bins, 0f32);
							let mut weights = vec::from_elem(data.bins, 0f32);
							let mut samples = data.samples;
							rand_var.clear();

							while running && samples > 0 {
								rand_var.clear();
								let frequency = min_freq + rand_var.next() * freq_diff;
								let sample_x = cam_x + (x as f32 - 0.5 + rand_var.next()) * pixel_size;
								let sample_y = cam_y + (y as f32 - 0.5 + rand_var.next()) * pixel_size;
								let mut ray = data.scene.get().camera.ray_to(sample_x, sample_y, &mut rand_var);

								let mut bounces = vec::with_capacity(10);
								let mut dispersion = false;

								for _ in range(0, 10) {
									match Tracer::trace(ray, frequency, data.scene.get(), &mut rand_var) {
										Some(reflection) => {
											let emission = reflection.emission;
											ray = reflection.out;
											dispersion = dispersion || reflection.dispersion;
											bounces.push(reflection);

											if emission {
												break;
											}
										},
										None => {
											//TODO: Background color
											/*bounces.push(Reflection {
												out: ray,
												color: 0.0,
												emission: true, 
												dispersion: false
											});*/
											break;
										}
									};
								}

								if bounces.len() > 0 && bounces.last().unwrap().emission {
									if dispersion {
										let value = bounces.iter().rev().fold(0.0, |incoming, ref reflection| {
											if reflection.emission {
												max(0.0, reflection.color.get(0.0, 0.0, frequency))
											} else {
												incoming * max(0.0, min(1.0, reflection.color.get(0.0, 0.0, frequency)))
											}
										});

										let bin = min(((frequency - min_freq) / freq_diff * data.bins as f32).floor() as uint, data.bins-1);
										values[bin] += value;
										weights[bin] += 1.0;
									} else {
										for bin in range(0, data.bins) {
											//TODO: Only change first value in rand_var
											let frequency = min_freq + freq_diff * (bin as f32 + rand_var.next()) / data.bins as f32;

											let value = bounces.iter().rev().fold(0.0, |incoming, ref reflection| {
												if reflection.emission {
													max(0.0, reflection.color.get(0.0, 0.0, frequency))
												} else {
													incoming * max(0.0, min(1.0, reflection.color.get(0.0, 0.0, frequency)))
												}
											});

											values[bin] += value;
											weights[bin] += 1.0;
										}
									}
								} else {
									if dispersion {
										let bin = min(((frequency - min_freq) / freq_diff * data.bins as f32).floor() as uint, data.bins-1);
										weights[bin] += 1.0;
									} else {
										for bin in range(0, data.bins) {
											weights[bin] += 1.0;
										}
									}
								}
						
								samples -= 1;
							}

							values.iter().zip(weights.iter()).map(|(&v, &w)| {
								if w == 0.0 {
									0.0
								} else {
									v/w
								}
							}).collect()
						}).collect();

						data.pixels.access(|&ref mut pixels| {
							for (i, &ref p) in tile_column.iter().enumerate() {
								let index = pix_x + x + (pix_y + i as u32) * image_w;
								pixels[index] = p.to_owned();
							}
						});
						task::deschedule();
					}
				},
				None => {running = false;}
			};

			match data.command_port.try_recv() {
				Data(Stop) => running = false,
				_=>{}
			};
		}

		data.task_counter.access(|&ref mut num| {
			**num -= 1;
		});
	}

	fn get_tile(tiles: &mut ~[Tile]) -> Option<Tile> {
		println!("{} tiles left", tiles.len());
		tiles.shift()
	}

	fn trace(ray: Ray, frequency: f32, scene: &Scene, rand_var: &mut RandomVariable) -> Option<Reflection> {
		let mut closest_dist = std::f32::INFINITY;
		let mut closest_hit = None;

		for object in scene.objects.iter() {
			match object.intersect(ray) {
				Some((hit, dist)) => {
					if dist < closest_dist && dist > 0.001 {
						closest_dist = dist;
						closest_hit = Some((object, Ray::new(hit.origin, hit.direction)));
					}
				},
				None => {}
			}
		}

		match closest_hit {
			Some((object, hit)) => {
				//Use object material to get emission, color and reflected ray
				let material = &scene.materials[object.get_material_index(hit, ray)];
				Some(material.get_reflection(hit, ray, frequency, rand_var))
			},
			None => None
		}
	}
}

struct TracerTask {
	command_chan: Chan<TracerCommands>,
}

impl TracerTask {
	pub fn stop(&self) {
		self.command_chan.send(Stop);
	}
}

enum TracerCommands {
	Stop
}

struct TracerData {
	command_port: Port<TracerCommands>,
	tiles: ~MutexArc<~[Tile]>,
	samples: u32,
	task_counter: ~MutexArc<~u16>,
	scene: ~Arc<Scene>,
	bins: uint,
	image_size: (u32, u32),
	pixels: ~MutexArc<~[~[f32]]>,
	freq_span: (f32, f32)
}


//Reflection
pub struct Reflection {
    out: Ray,
    color: ~ParametricValue,
    emission: bool,
    dispersion: bool
}


//Tiles
struct Tile {
	screen: (u32, u32, u32, u32),
	world: (f32, f32, f32)
}

impl Clone for Tile {
	fn clone(&self) -> Tile {
		Tile {
			screen: self.screen,
			world: self.world
		}
	}
}

pub fn generate_tiles(image_size: (u32, u32), tile_size: (u32, u32)) -> ~[Tile] {
	let mut y = 0;
	let (image_w, image_h) = image_size;
	let max_size = max(image_w, image_h) as f32;
	let norm_w = image_w as f32 / max_size;
	let norm_h = image_h as f32 / max_size;
	let (tile_w, tile_h) = tile_size;
	let mut tiles = ~[];

	while y < image_h {
		let h = min(tile_h, image_h - y);
		let mut x = 0;
		while x < image_w {
			let w = min(tile_w, image_w - x);
			tiles.push(Tile{
				screen: (x, y, w, h),
				world: (
					(2*x) as f32 / max_size - norm_w,
					(2*y) as f32 / max_size - norm_h,
					2.0 / max_size
				)
			});
			x += tile_w;
		}
		y+= tile_h;
	}

	tiles.sort_by(|&a, &b| {
		let (a_x, a_y, _) = a.world;
		let (b_x, b_y, _) = b.world;
		let a_dist = a_x * a_x + a_y * a_y;
		let b_dist = b_x * b_x + b_y * b_y;
		if a_dist < b_dist { Less }
		else if a_dist > b_dist { Greater }
		else { Equal }
	});
	tiles
}

//Ray
pub struct Ray {
    origin: Vec3<f32>,
    direction: Vec3<f32>
}

impl Ray {
	pub fn new(origin: Vec3<f32>, direction: Vec3<f32>) -> Ray {
		Ray{origin: origin, direction: na::normalize(&direction)}
	}

	pub fn to(from: Vec3<f32>, to: Vec3<f32>) -> Ray{
		Ray{origin: from, direction: na::normalize(&(to - from))}
	}
}


//Scene Object
pub trait SceneObject: Send+Freeze {
	fn get_material_index(&self, normal: Ray, ray_in: Ray) -> uint;
	fn get_proximity(&self, ray: Ray) -> Option<f32>;
	fn intersect(&self, ray: Ray) -> Option<(Ray, f32)>;
}


//Camera
pub struct Camera {
	position: Vec3<f32>,
	rotation: Rot3<f32>,
	lens: f32,
	aperture: f32,
	focal_distance: f32
}

impl Camera {
	pub fn new(position: Vec3<f32>, rotation: Vec3<f32>) -> Camera {
		Camera{
			position: position,
			rotation: Rot3::new(rotation),
			lens: 2.0,
			aperture: 0.0,
			focal_distance: 0.0
		}
	}

	pub fn look_at(from: Vec3<f32>, to: Vec3<f32>, up: Vec3<f32>) -> Camera {
		let mut rot = Rot3::new(na::zero());
		rot.look_at_z(&(to - from), &up);
		Camera{
			position: from,
			rotation: rot,
			lens: 2.0,
			aperture: 0.0,
			focal_distance: 0.0
		}
	}

	fn ray_to(&self, x: f32, y: f32, rand_var: &mut RandomVariable) -> Ray {
		if self.aperture == 0.0 {
			Ray::new(self.position, self.rotation.rotate(&Vec3::new(x, -y, -self.lens)))
		} else {
			let base_dir = Vec3::new(x / self.lens, -y / self.lens, -1.0);
			let focal_point = base_dir * self.focal_distance;

			let sqrt_r = sqrt(rand_var.next() * self.aperture);
			let psi = rand_var.next() * 2.0 * std::f32::consts::PI;
			let lens_x = sqrt_r * psi.cos();
			let lens_y = sqrt_r * psi.sin();

			let lens_point = Vec3::new(lens_x, lens_y, 0.0);
			
			Ray::new(self.rotation.rotate(&lens_point) + self.position, self.rotation.rotate(&(focal_point - lens_point)))
		}
	}
}

//Scene
pub struct Scene {
	camera: Camera,
	objects: ~[~SceneObject: Send + Freeze],
	materials: ~[~Material: Send + Freeze]
}

//Material
pub trait Material {
	fn get_reflection(&self, normal: Ray, ray_in: Ray, frequency: f32, rand_var: &mut RandomVariable) -> Reflection;
	fn to_owned_material(&self) -> ~Material: Send+Freeze;
}

//Parametric value
pub trait ParametricValue {
	fn get(&self, x: f32, y: f32, i: f32) -> f32;
	fn clone_value(&self) -> ~ParametricValue: Send+Freeze;
}