extern mod png;
extern mod extra;
extern mod nalgebra;
use std::num::min;
use std::io::{File, io_error};
use extra::time::precise_time_s;
use extra::json;
use nalgebra::na;
use nalgebra::na::{Vec3, Rot3};
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

	let project_dir = if project_file.len() == 0 {
		Path::new("./")
	} else {
		Path::new(project_file.to_owned()).with_filename("")
	};

	println!("Project directory: {}", project_dir.display());

	let project = load_project(project_file);

	let mut tracer = build_project(project);
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
				save_png(project_dir.with_filename("render.png"), values, width, height);
			});
			last_image_update = precise_time_s();
		}
		std::task::deschedule();
	}

	println!("Render time: {}s", precise_time_s() - render_started);

	tracer.pixels.access(|&ref mut values| {
		let (width, height) = tracer.image_size;
		save_png(project_dir.with_filename("render.png"), values, width, height);
	});
}
	

fn save_png(path: Path, values: &~[~[f32]], width: u32, height: u32) {
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

	png::store_png(&image, &path);
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
		_ => println!("Warning: No valid render configurations provided")
	}

	tracer.set_scene(scene_from_json(project));

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

fn scene_from_json(config: &json::Object) -> Scene {
	Scene {
		camera: camera_from_json(config),
		objects: objects_from_json(config)
	}
}

fn camera_from_json(config: &json::Object) -> Camera {
	let mut camera = Camera::new(na::zero(), na::zero());

	match config.find(&~"camera") {
		Some(&json::Object(ref camera_cfg)) => {
			match camera_cfg.find(&~"position") {
				Some(&json::List(ref position)) => {
					if(position.len() == 3) {
						match position[0] {
							json::Number(x) => {
								camera.position.x = x as f32;
							},
							_ => println!("Warning: Camera position must be a list of 3 numbers")
						}

						match position[1] {
							json::Number(y) => {
								camera.position.y = y as f32;
							},
							_ => println!("Warning: Camera position must be a list of 3 numbers")
						}

						match position[2] {
							json::Number(z) => {
								camera.position.z = z as f32;
							},
							_ => println!("Warning: Camera position must be a list of 3 numbers")
						}
					} else {
						println!("Warning: Camera position must be a list of 3 numbers");
					}
				},
				_ => {}
			}

			match camera_cfg.find(&~"rotation") {
				Some(&json::List(ref rotation)) => {
					let mut new_rotation: Vec3<f32> = na::zero();

					if(rotation.len() == 3) {
						match rotation[0] {
							json::Number(x) => {
								new_rotation.x = x as f32;
							},
							_ => println!("Warning: Camera rotation must be a list of 3 numbers")
						}

						match rotation[1] {
							json::Number(y) => {
								new_rotation.y = y as f32;
							},
							_ => println!("Warning: Camera rotation must be a list of 3 numbers")
						}

						match rotation[2] {
							json::Number(z) => {
								new_rotation.z = z as f32;
							},
							_ => println!("Warning: Camera rotation must be a list of 3 numbers")
						}
					} else {
						println!("Warning: Camera rotation must be a list of 3 numbers");
					}

					camera.rotation = Rot3::new(new_rotation);
				},
				_ => {}
			}

			match camera_cfg.find(&~"lens") {
				Some(&json::Number(lens)) => {
					camera.lens = lens as f32;
				},
				_ => {}
			}

			match camera_cfg.find(&~"aperture") {
				Some(&json::Number(aperture)) => {
					camera.aperture = aperture as f32;
				},
				_ => {}
			}

			match camera_cfg.find(&~"focal_distance") {
				Some(&json::Number(focal_distance)) => {
					camera.focal_distance = focal_distance as f32;
				},
				_ => {}
			}
		},
		_ => println!("Warning: No valid camera configuration provided")
	}

	camera
}

fn objects_from_json(config: &json::Object) -> ~[~SceneObject: Send+Freeze] {
	let default_material = ~materials::Diffuse{
		color: 0.8
	} as ~Material: Send+Freeze;

	let materials = match config.find(&~"materials") {
		Some(&json::List(ref materials)) => {
			materials.iter().filter_map(|o| {
				match o {
					&json::Object(ref material) => {
						materials::from_json(material)
					},
					_ => None
				}
			}).collect()
		},
		_ => ~[]
	};

	match config.find(&~"objects") {
		Some(&json::List(ref objects)) => {
			objects.iter().filter_map(|o| {
				match o {
					&json::Object(ref object) => {
						match object.find(&~"type") {
							Some(&json::String(~"Sphere")) => {
								Some(~Sphere::from_json(object, materials, default_material) as ~SceneObject: Send+Freeze)
							},
							_ => None
						}
					},
					_ => None
				}
			}).collect()
		},
		_ => ~[]
	}
}