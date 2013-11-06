extern mod png;
extern mod extra;
extern mod nalgebra;
use std::vec;
use std::num::min;
use extra::time::precise_time_s;
use nalgebra::na::Vec3;
use core::{Tracer, Camera, Scene, SceneObject, Material};
use shapes::Sphere;
mod core;
mod shapes;
mod materials;

fn main() {
	let width = 512;
	let height = 512;

	let mut spheres = vec::from_fn(20, |i| {
		let x = if i < 10 { -2.0 } else { 2.0 };
		let z = (if i < 10 { i } else { i - 10 } as f32 * 5.0) + 3.0;
		let material = if i % 2 == 0 {
			~materials::Mirror {
				color: 1.0
			} as ~Material: Send+Freeze
		} else {
			~materials::Diffuse {
				color: 0.0,
				emission: 1.5
			} as ~Material: Send+Freeze
		};
		~Sphere::new(Vec3::new(x, 0.0, 1.0 + z), 1.0, material) as ~SceneObject: Send+Freeze
	});

	let material = ~materials::Diffuse {
		color: 0.5,
		emission: 0.0
	};

	spheres.push(~Sphere::new(Vec3::new(0.0, 101.0, 5.0), 100.0, material as ~Material: Send+Freeze) as ~SceneObject: Send+Freeze);

	let scene = Scene {
		camera: Camera::new(Vec3::new(5.0, -3.0, -4.0), Vec3::new(-0.3, -0.4, 0.0)),
		objects: spheres
	};

	let mut tracer = Tracer::new();
	tracer.samples = 100;
	tracer.image_size = (width, height);
	tracer.set_scene(scene);
	tracer.bins = 3;

	let render_started = precise_time_s();

	let mut tracers = ~[];
	std::task::deschedule();
	for n in std::iter::range(0, 4) {
		println!("Starting render task {}", n);
		tracers.push(tracer.spawn());
		std::task::deschedule();
	}


	let mut last_image_update = precise_time_s();
	while !tracer.done() {
		//Don't be too eager!
		if(!tracer.done()) {
			std::rt::io::timer::sleep(500);
		}

		if last_image_update < precise_time_s() - 5.0 {
			tracer.pixels.access(|&ref mut values| {
				save_png(values, width, height);
			});
			last_image_update = precise_time_s();
		}
		std::task::deschedule();
	}

	println!("Render time: {}s", precise_time_s() - render_started);

	tracer.pixels.access(|&ref mut values| {
		save_png(values, width, height);
	});
}
	

fn save_png(values: &~[~[f32]], width: u32, height: u32) {
	println!("Saving PNG...");
	let pixels: ~[~[u8]] = values.iter().map(|ref values| {
		values.iter().map(|&v| {
			(min(v, 1.0) * 255.0) as u8
		}).collect()
	}).collect();

	let image = png::Image{
		width: width,
		height: height,
		color_type: png::RGB8,
		pixels: pixels.concat_vec()
	};

	png::store_png(&image, &Path::new("test.png"));
}