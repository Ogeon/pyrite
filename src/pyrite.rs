extern mod png;
extern mod extra;
extern mod nalgebra;
use std::vec;
use std::num::min;
use std::io::{File, io_error};
use extra::time::precise_time_s;
use extra::json;
use extra::json::ToJson;
use nalgebra::na;
use nalgebra::na::Vec3;
use core::{Tracer, Camera, Scene, SceneObject, Material};
use shapes::Sphere;
mod core;
mod shapes;
mod materials;

fn main() {
	let mut render_only = false;
	let mut project_file = ~"";

	let args = std::os::args();
	for arg in args.iter().skip(1) {
		match arg {
			&~"--render" | &~"-r" => render_only = true,
			file_name => project_file = file_name.to_owned()
		}
	}

	let project = load_project(project_file);

	println!("Current project:\n{}", project.to_json().to_pretty_str());


	let mut spheres = vec::from_fn(20, |i| {
		let x = if i < 10 { -2.0 } else { 2.0 };
		let z = (if i < 10 { i } else { i - 10 } as f32 * 5.0) + 3.0;
		let material = if i % 2 == 0 {
			let a = ~materials::Mirror {
				color: 1.0
			} as ~Material: Send+Freeze;
			let b = ~materials::Diffuse {
				color: 1.0
			} as ~Material: Send+Freeze;
			~materials::FresnelMix {
				reflection: a,
				refraction: b,
				refractive_index: 1.5,
				dispersion: 0.0
			} as ~Material: Send+Freeze
			
		} else {
			~materials::Emission {
				color: 1.0,
				luminance: 3.0
			} as ~Material: Send+Freeze
		};
		~Sphere::new(Vec3::new(x, 0.0 - (0 * (i%10)) as f32, 1.0 + z), 1.0, material) as ~SceneObject: Send+Freeze
	});

	let material = ~materials::Diffuse {
		color: 0.7
	};

	spheres.push(~Sphere::new(Vec3::new(0.0, 101.0, 5.0), 100.0, material as ~Material: Send+Freeze) as ~SceneObject: Send+Freeze);

	let mut scene = Scene {
		camera: Camera::new(Vec3::new(5.0, -3.0, -4.0), Vec3::new(-0.3, -0.4, 0.0)),
		objects: spheres
	};

	scene.camera.focal_distance = na::norm(&(scene.camera.position - Vec3::new(-2.0f32, 0.0, 4.0)));//7.0;
	scene.camera.aperture = 0.02;

	let mut tracer = build_project(project);
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
			std::io::timer::sleep(500);
		}

		if last_image_update < precise_time_s() - 5.0 {
			tracer.pixels.access(|&ref mut values| {
				let (width, height) = tracer.image_size;
				save_png(values, width, height);
			});
			last_image_update = precise_time_s();
		}
		std::task::deschedule();
	}

	println!("Render time: {}s", precise_time_s() - render_started);

	tracer.pixels.access(|&ref mut values| {
		let (width, height) = tracer.image_size;
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

fn load_project(path: &str) -> ~json::Object {
	let default = "{\"objects\": [], \"cameras\": [], \"materials\": [], \"render\": {}}";

	let mut project = if path.len() == 0 {
		//No file provided
		println!("New project created");
		json::from_str(default)
	} else {
		do io_error::cond.trap(|error| {
			//Catching io_error
			println!("Unable to open {}: {}", path, error.desc);
		}).inside {
			//Open provided file
			match File::open(&Path::new(path)) {
				//A valid path was provided
				Some(mut file) => json::from_reader(&mut file as &mut std::io::Reader),

				//An invalid path was provided
				None => {
					println!("New project created");
					json::from_str(default)
				}
			}
		}
	};

	if project.is_err() {
		//Errors while parsing the JSON data
		println!("Error parsing file: {}", project.unwrap_err().to_str());
		project = json::from_str(default);
	}

	//Check if the root is an object and extract it
	match project.unwrap() {
		json::Object(result) => return result,
		_ => println!("Project root must be an object")
	}

	//The root was something else
	println!("New project created");
	project = json::from_str(default);

	//Extract the default root object
	match project.unwrap() {
		json::Object(result) => return result,
		_ => fail!("This is a bug. The default project is invalid")
	}
}

fn build_project(project: &json::Object) -> Tracer {
	let mut tracer = Tracer::new();

	match project.find(&~"render") {
		Some(&json::Object(ref render_cfg)) => {
			tracer_from_json(render_cfg, &mut tracer);
		},
		_ => println!("No valid render configurations provided")
	}

	tracer
}

fn tracer_from_json(config: &~json::Object, tracer: &mut Tracer) {
	match config.find(&~"width") {
		Some(&json::Number(width)) => {
			let (_, height) = tracer.image_size;
			tracer.image_size = (width as u32, height);
		},
		_ => {}
	}

	match config.find(&~"height") {
		Some(&json::Number(height)) => {
			let (width, _) = tracer.image_size;
			tracer.image_size = (width, height as u32);
		},
		_ => {}
	}
	
	match config.find(&~"samples") {
		Some(&json::Number(samples)) => {
			tracer.samples = samples as u32;
		},
		_ => {}
	}
	
	match config.find(&~"tile_width") {
		Some(&json::Number(width)) => {
			let (_, height) = tracer.tile_size;
			tracer.tile_size = (width as u32, height);
		},
		_ => {}
	}

	match config.find(&~"tile_height") {
		Some(&json::Number(height)) => {
			let (width, _) = tracer.tile_size;
			tracer.tile_size = (width, height as u32);
		},
		_ => {}
	}
}