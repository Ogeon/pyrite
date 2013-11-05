extern mod std;
use std::rand::Rng;
use std::num::{exp, ln, min, max};
use std::{task, fmt, vec};
use std::comm::stream;
use std::iter::range;
use extra::arc::{MutexArc, Arc};
use extra::sort::merge_sort;
use nalgebra::na::{Vec3, Rot3, Rotate};
use nalgebra::na;

//Random variable

struct RandomVariable {
	values: ~[f32],
	pos: uint
}

impl RandomVariable {
	pub fn new() -> RandomVariable {
		RandomVariable{
			values: ~[],
			pos: 0
		}
	}

	pub fn next<T: Rng>(&mut self, random: &mut T) -> f32 {
		let pos = self.pos;
		self.pos += 1;

		while pos >= self.values.len() {
			self.values.push(random.gen());
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
trait Sampler {
	fn random_variable<T: Rng>(random: &mut T) -> RandomVariable;
	fn sample(traced: Sample) -> Sample;
}

//Tracer
struct Tracer {
	samples: u32,
	active_tasks: ~MutexArc<~u16>,
	scene: ~Arc<Scene>,
	image_size: (u32, u32),
	tile_size: (u32, u32),
	tiles: ~MutexArc<~[Tile]>,
	bins: uint,
	pixels: ~MutexArc<~[~[f32]]>
}

impl Tracer {
	pub fn new() -> Tracer {
		Tracer {
			samples: 10,
			active_tasks: ~MutexArc::new(~0),
			scene: ~Arc::new(Scene{
				camera: Camera::new(na::zero(), na::zero()),
				objects: ~[]
			}),
			image_size: (512, 512),
			tile_size: (64, 64),
			tiles: ~MutexArc::new(~[]),
			bins: 3,
			pixels: ~MutexArc::new(vec::from_elem(512 * 512, vec::from_elem(3, 0.0f32)))
		}
	}

	pub fn spawn<R: Rng + Send/*, S: Sampler*/>(&mut self, random: R/*, sampler: S*/) -> TracerTask {
		if self.done() {
			let (image_w, image_h) = self.image_size;
			self.tiles = ~MutexArc::new(generate_tiles(self.image_size, self.tile_size));
			self.pixels = ~MutexArc::new(vec::from_elem((image_w * image_h) as uint, vec::from_elem(self.bins, 0.0f32)));
		}

		let (command_port, command_chan) = stream::<TracerCommands>();
		let data = TracerData{
			random: random,
			command_port: command_port,
			tiles: self.tiles.clone(),//generate_tiles(self.image_size, self.tile_size),
			samples: self.samples,
			task_counter: self.active_tasks.clone(),
			scene: self.scene.clone(),
			bins: self.bins,
			image_size: self.image_size,
			pixels: self.pixels.clone()
			//sampler: sampler
		};

		let task_number = self.active_tasks.access(|&ref mut num| {
			**num += 1;
			**num-1
		});
		let mut new_task = task::task();
		//new_task.sched_mode(DefaultScheduler);
		new_task.unlinked();
		new_task.indestructible();
		new_task.name(format!("Task {}", task_number));
		new_task.spawn_with(data, Tracer::run);

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

	fn run<R: Rng + Send>(mut data: TracerData<R>) {
		task::deschedule();

		let mut running = true;
		let mut rand_var = RandomVariable::new();
		//let id: u16 = data.random.gen();
		let (image_w, _) = data.image_size;

		while running {
			let maybe_tile = data.tiles.access(Tracer::get_tile);

			match maybe_tile {
				Some(tile) => {
					let (pix_x, pix_y, pix_w, pix_h) = tile.screen;
					let (cam_x, cam_y, pixel_size) = tile.world;
					//let mut results = vec::with_capacity((pix_w*pix_h) as uint);
					
					for x in range(0, pix_w) {
						let tile_column: ~[~[f32]] = range(0, pix_h).map(|y| {
							let mut values = vec::from_elem(data.bins, 0f32);
							let mut weights = vec::from_elem(data.bins, 0f32);
							let mut samples = data.samples;
							rand_var.clear();

							while running && samples > 0 {
								rand_var.clear();
								let frequency = rand_var.next(&mut data.random);
								let sample_x = cam_x + (x as f32 - 0.5 + rand_var.next(&mut data.random)) * pixel_size;
								let sample_y = cam_y + (y as f32 - 0.5 + rand_var.next(&mut data.random)) * pixel_size;
								let mut ray = data.scene.get().camera.ray_to(sample_x, sample_y);

								let mut bounces = vec::with_capacity(10);

								for _ in range(0, 10) {
									match Tracer::trace(ray, data.scene.get(), &mut rand_var, &mut data.random) {
										Some(reflection) => {
											ray = reflection.out;
											bounces.push(reflection);
										},
										None => {
											bounces.push(Reflection {
												out: ray,
												absorbation: 0.0,
												emission: frequency*2.0 //TODO: Background color
											});
											break;
										}
									};
								}

								let value = bounces.iter().invert().fold(0.0, |incoming, &reflection| {
									incoming * reflection.absorbation + reflection.emission
								});

								let bin = min((frequency * data.bins as f32).floor() as uint, data.bins-1);
								values[bin] += value;
								weights[bin] += 1.0;
						
								samples -= 1;
							}

							values.iter().zip(weights.iter()).map(|(&v, &w)| {
								if(w == 0.0) {
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

					std::rt::io::timer::sleep(0);
				},
				None => {running = false;}
			};

			if data.command_port.peek() {
				match data.command_port.recv() {
					Stop => running = false
				}
			}
		}

		data.task_counter.access(|&ref mut num| {
			**num -= 1;
		});
	}

	fn get_tile(tiles: &mut ~[Tile]) -> Option<Tile> {
		if(tiles.len() > 0) {
			Some(tiles.shift())
		} else {
			None
		}
	}

	fn trace<R: Rng>(ray: Ray, scene: &Scene, rand_var: &mut RandomVariable, random: &mut R) -> Option<Reflection> {
		let mut hits = ~[];

		//Find possible hits
		for object in scene.objects.iter() {
			match object.get_bounds().intersect(ray) {
				Some(d) => hits.push((object, d)),
				None => {}
			};
		}


		let mut closest_dist = std::f32::INFINITY;
		let mut closest_hit = None;

		//Find closest hit
		for &(ref object, d) in hits.iter() {
			if d < closest_dist {
				match object.intersect(ray) {
					Some((hit, dist)) => {
						if(dist < closest_dist && dist > 0.0002) {
							closest_dist = dist;
							closest_hit = Some((object, Ray::new(hit.origin, hit.direction)));
						}
					},
					None => {}
				}
			}
		}

		match closest_hit {
			Some((object, hit)) => {
				//Use material to get emission, absorbation and reflected ray
				//TODO: Actually use materials

				let u = rand_var.next(random);
				let v = rand_var.next(random);
				let theta = 2.0 * std::f32::consts::PI * u;
				let phi = std::num::acos(2.0 * v - 1.0);
				let sphere_point = Vec3::new(
					phi.sin() * theta.cos(),
					phi.sin() * theta.sin(),
					phi.cos().abs()
					);

				let mut bases = vec::with_capacity(3);

				na::orthonormal_subspace_basis(&hit.direction, |base| {
					bases.push(base);
					true
				});
				bases.push(hit.direction);

				let mut reflection: Vec3<f32> = na::zero();

				unsafe {
					for (i, base) in bases.iter().enumerate() {
						reflection = reflection + base * sphere_point.at_fast(i);
					}
				}

				let out = Ray::new(hit.origin, reflection);
				let absorbation = 0.5;
				let emission = 0.0;
				Some(Reflection {
					out: out,
					absorbation: absorbation,
					emission: emission
				})
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

struct TracerData<R/*, S*/> {
	random: R,
	command_port: Port<TracerCommands>,
	tiles: ~MutexArc<~[Tile]>,
	samples: u32,
	task_counter: ~MutexArc<~u16>,
	scene: ~Arc<Scene>,
	bins: uint,
	image_size: (u32, u32),
	pixels: ~MutexArc<~[~[f32]]>
	//sampler: S
}


//Reflection
struct Reflection {
    out: Ray,
    absorbation: f32,
    emission: f32
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

	merge_sort(tiles, |&a, &b| {
		let (a_x, a_y, _) = a.world;
		let (b_x, b_y, _) = b.world;
		(a_x * a_x + a_y * a_y) <= (b_x * b_x + b_y * b_y)
	})
}

//Ray
struct Ray {
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

//Bounding Box
struct BoundingBox {
	from: Vec3<f32>,
	to: Vec3<f32>
}

impl BoundingBox {
	fn intersect(&self, ray: Ray) -> Option<f32> {
		let origin = ray.origin;
		let dir = ray.direction;

		let mut quadrant = [Left, Left, Left];
		let mut candidate_plane = [0f32, ..3];
		let mut inside = true;

		let mut coord: Vec3<f32> = na::zero();
		let mut max_t = [0f32, ..3];
		let mut witch_plane = 0;

		unsafe {
			for i in range(0 as uint, 3) {
				if origin.at_fast(i) < self.from.at_fast(i) {
					candidate_plane[i] = self.from.at_fast(i);
					inside = false;
				} else if origin.at_fast(i) > self.to.at_fast(i) {
					quadrant[i] = Right;
					candidate_plane[i] = self.to.at_fast(i);
					inside = false;
				} else {
					quadrant[i] = Middle;
				}
			}
		}

		if inside {
			return Some(0.0);
		}

		unsafe {
			for i in range(0 as uint, 3) {
				if quadrant[i] != Middle && dir.at_fast(i) != 0.0 {
					max_t[i] = (candidate_plane[i] - origin.at_fast(i)) / dir.at_fast(i);
				} else {
					max_t[i] = -1.0;
				}
			}
		}

		for (i, &v) in max_t.iter().enumerate() {
			if v > max_t[witch_plane] {
				witch_plane = i;
			}
		}

		if max_t[witch_plane] < 0.0 {
			return None;
		}

		unsafe {
			for i in range(0 as uint, 3) {
				if(witch_plane != i) {
					coord.set_fast(i, origin.at_fast(i) + max_t[witch_plane] * dir.at_fast(i));
					if coord.at_fast(i) < self.from.at_fast(i) || coord.at_fast(i) > self.to.at_fast(i) {
						return None;
					}
				} else {
					coord.set_fast(i, candidate_plane[i]);
				}
			}
		}

		return Some(na::norm(&(coord - origin)));
	}
}

enum Quadrant {
	Left = 0,
	Middle = 1,
	Right = 2
}

impl Eq for Quadrant {
	fn eq(&self, other: &Quadrant) -> bool {
		*self as int == *other as int
	}

	fn ne(&self, other: &Quadrant) -> bool {
		*self as int != *other as int
	}
}


//Scene Object
pub trait SceneObject: Send+Freeze {
	fn get_bounds(&self) -> BoundingBox;
	fn intersect(&self, ray: Ray) -> Option<(Ray, f32)>;
}


//
pub struct Camera {
	position: Vec3<f32>,
	rotation: Rot3<f32>,
	lens: f32
}

impl Camera {
	pub fn new(position: Vec3<f32>, rotation: Vec3<f32>) -> Camera {
		Camera{position: position, rotation: Rot3::new(rotation), lens: 2.0}
	}

	pub fn look_at(from: Vec3<f32>, to: Vec3<f32>, up: Vec3<f32>) -> Camera {
		let mut rot = Rot3::new(na::zero());
		rot.look_at_z(&(to - from), &up);
		Camera{position: from, rotation: rot, lens: 2.0}
	}

	fn ray_to(&self, x: f32, y: f32) -> Ray {
		Ray::new(self.position, self.rotation.rotate(&Vec3::new(x, y, self.lens)))
	}
}

//Scene
pub struct Scene {
	camera: Camera,
	objects: ~[~SceneObject: Send + Freeze]
}