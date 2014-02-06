extern mod png;
extern mod extra;
extern mod nalgebra;
use std::num::{min, max};
use std::io::{File, io_error, stdio, Reader};
use std::io::BufferedReader;
use std::str::StrSlice;
use extra::time::precise_time_s;
use extra::json;
use nalgebra::na;
use nalgebra::na::{Vec3, Rot3};
use core::{Tracer, Camera, Scene, SceneObject, Material, ParametricValue};
mod core;
mod shapes;
mod materials;
mod values;

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
		Path::new(project_file.to_owned())
	};

	println!("Project path: {}", project_dir.display());

	let project = load_project(project_file);

	if render_only {
		let tracer = render(project, &project_dir);
		let response_curves = get_response_curves(project);

		tracer.pixels.access(|&ref mut values| {
			let (width, height) = tracer.image_size;
			save_png(project_dir.with_filename("render.png"), values, width, height, &response_curves);
		});
	} else {
		let mut stdin = BufferedReader::new(stdio::stdin());

		loop {
			print!("> ");
			stdio::flush();
			match stdin.read_line() {
				Some(line) => {
					let args: ~[&str] = line.trim().splitn(' ', 1).collect();
					match args {
						[&"render"] => {
							let tracer = render(project, &project_dir);
							let response_curves = get_response_curves(project);

							tracer.pixels.access(|&ref mut values| {
								let (width, height) = tracer.image_size;
								save_png(project_dir.with_filename("render.png"), values, width, height, &response_curves);
							});
						},
						[&"get"] => {
							println!("Type \"get path.to.something\" to get the value of \"something\"")
						},
						[&"get", path] => {
							let project_object = json::Object(project.clone());
							match path.split('.').fold(Some(&project_object), |result, key| {
								let k = key.to_owned();
								match result {
									Some(&json::Object(ref map)) => {
										map.find(&k)
									},
									_ => None
								}
							}) {
								Some(object) => println!("{}", object.to_pretty_str()),
								None => println!("Could not find \"{}\" in the project", path)
							}
						},
						[&"quit"] => break,
						[&"exit"] => break,
						_ => println!("Unknown command \"{}\"", line.trim())
					}
				},
				None => break
			}
		}
	}
}

fn render(project: &json::Object, path: &Path) -> ~Tracer {
	let mut tracer = ~build_project(project);
	tracer.bins = 40;

	let mut tracers = ~[];
	std::task::deschedule();
	for n in std::iter::range(0, 4) {
		println!("Starting render task {}", n);
		tracers.push(tracer.spawn());
		std::task::deschedule();
	}

	let response_curves = get_response_curves(project);

	let render_started = precise_time_s();

	let mut last_image_update = precise_time_s();
	while !tracer.done() {
		//Don't be too eager!
		if !tracer.done() {
			std::io::timer::sleep(500);
		}

		if last_image_update < precise_time_s() - 60.0 {
			tracer.pixels.access(|&ref mut values| {
				let (width, height) = tracer.image_size;
				save_png(path.with_filename("render.png"), values, width, height, &response_curves);
			});
			last_image_update = precise_time_s();
		}
		std::task::deschedule();
	}

	println!("Render time: {}s", precise_time_s() - render_started);

	tracer
}
	

fn save_png(path: Path, values: &~[~[f32]], width: u32, height: u32, response: &[~ParametricValue, ..3]) {
	let min_freq = 400.0;
	let max_freq = 740.0;

	println!("Saving {}...", path.as_str().unwrap_or("rendered image"));
	let pixels: ~[~[u8]] = values.iter().map(|values| {
		freq_to_rgb((min_freq, max_freq), values, response)
	}).collect();

	let image = png::Image{
		width: width,
		height: height,
		color_type: png::RGB8,
		pixels: pixels.concat_vec()
	};

	png::store_png(&image, &path);
}

fn freq_to_rgb(freq_span: (f32, f32), color: &~[f32], response: &[~ParametricValue, ..3]) -> ~[u8] {
	let (min_freq, max_freq) = freq_span;
	let freq_diff = max_freq - min_freq;
	let bin_width = freq_diff / color.len() as f32;

	let (rv, rw) = color.iter().enumerate().fold((0.0, 0.0), |(sum_v, sum_w), (i, v)| {
		let start = min_freq + i as f32 * bin_width;
		let end = min_freq + (i + 1) as f32 * bin_width;
		let w = (max(0.0, response[0].get(0.0, 0.0, start)) + max(0.0, response[0].get(0.0, 0.0, end))) / 2.0;
		(v * w + sum_v, w + sum_w)
	});

	let r = min( 1.0, (if rw > 0.0 {rv / rw} else {0.0})) * 255.0;

	let (gv, gw) = color.iter().enumerate().fold((0.0, 0.0), |(sum_v, sum_w), (i, v)| {
		let start = min_freq + i as f32 * bin_width;
		let end = min_freq + (i + 1) as f32 * bin_width;
		let w = (max(0.0, response[1].get(0.0, 0.0, start)) + max(0.0, response[1].get(0.0, 0.0, end))) / 2.0;
		(v * w + sum_v, w + sum_w)
	});

	let g = min( 1.0, (if gw > 0.0 {gv / gw} else {0.0})) * 255.0;

	let (bv, bw) = color.iter().enumerate().fold((0.0, 0.0), |(sum_v, sum_w), (i, v)| {
		let start = min_freq + i as f32 * bin_width;
		let end = min_freq + (i + 1) as f32 * bin_width;
		let w = (max(0.0, response[2].get(0.0, 0.0, start)) + max(0.0, response[2].get(0.0, 0.0, end))) / 2.0;
		(v * w + sum_v, w + sum_w)
	});

	let b = min( 1.0, (if bw > 0.0 {bv / bw} else {0.0})) * 255.0;

	~[r as u8, g as u8, b as u8]
}
	

fn save_png_u8(path: Path, pixels: ~[u8], width: u32, height: u32) {
	println!("Saving PNG...");

	let image = png::Image{
		width: width,
		height: height,
		color_type: png::RGB8,
		pixels: pixels
	};

	png::store_png(&image, &path);
}

fn load_project(path: &str) -> ~json::Object {
	let default = "{\"objects\": [], \"camera\": {}, \"materials\": [], \"render\": {}}";

	let mut project = if path.len() == 0 {
		//No file provided
		println!("New project created");
		json::from_str(default)
	} else {
		io_error::cond.trap(|error| {
			//Catching io_error
			println!("Unable to open {}: {}", path, error.desc);
		}).inside(proc() {
			//Open provided file
			match File::open(&Path::new(path)) {
				//A valid path was provided
				Some(mut file) => json::from_reader(&mut file as &mut Reader),

				//An invalid path was provided
				None => {
					println!("New project created");
					json::from_str(default)
				}
			}
		})
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

fn get_response_curves(project: &json::Object) -> [~ParametricValue, ..3] {
	match project.find(&~"response") {
		Some(&json::Object(ref curves)) => {
			let r = match curves.find(&~"red") {
				Some(red_response) => {
					match values::from_json(red_response) {
						Some(value) => value,
						None => ~values::Number{value: 1.0} as ~ParametricValue
					}
				},
				None => ~values::Number{value: 1.0} as ~ParametricValue
			};
			let g = match curves.find(&~"green") {
				Some(green_response) => {
					match values::from_json(green_response) {
						Some(value) => value,
						None => ~values::Number{value: 1.0} as ~ParametricValue
					}
				},
				None => ~values::Number{value: 1.0} as ~ParametricValue
			};
			let b = match curves.find(&~"blue") {
				Some(blue_response) => {
					match values::from_json(blue_response) {
						Some(value) => value,
						None => ~values::Number{value: 1.0} as ~ParametricValue
					}
				},
				None => ~values::Number{value: 1.0} as ~ParametricValue
			};
			[r, g, b]
		},
		_ => [
			~values::Number{value: 1.0} as ~ParametricValue,
			~values::Number{value: 1.0} as ~ParametricValue,
			~values::Number{value: 1.0} as ~ParametricValue
		]
	}
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
	let materials = materials_from_json(config);
	Scene {
		camera: camera_from_json(config),
		objects: objects_from_json(config, materials.len() - 1),
		materials: materials
	}
}

fn camera_from_json(config: &json::Object) -> Camera {
	let mut camera = Camera::new(na::zero(), na::zero());

	match config.find(&~"camera") {
		Some(&json::Object(ref camera_cfg)) => {
			match camera_cfg.find(&~"position") {
				Some(&json::List(ref position)) => {
					if position.len() == 3 {
						match position[0] {
							json::Number(x) => {
								camera.position.x = x as f32;
							},
							_ => println!("Warning: Camera position must be a list of 3 numbers. Default will be used.")
						}

						match position[1] {
							json::Number(y) => {
								camera.position.y = y as f32;
							},
							_ => println!("Warning: Camera position must be a list of 3 numbers. Default will be used.")
						}

						match position[2] {
							json::Number(z) => {
								camera.position.z = z as f32;
							},
							_ => println!("Warning: Camera position must be a list of 3 numbers. Default will be used.")
						}
					} else {
						println!("Warning: Camera position must be a list of 3 numbers. Default will be used.");
					}
				},
				_ => {}
			}

			match camera_cfg.find(&~"rotation") {
				Some(&json::List(ref rotation)) => {
					let mut new_rotation: Vec3<f32> = na::zero();

					if rotation.len() == 3 {
						match rotation[0] {
							json::Number(x) => {
								new_rotation.x = x as f32;
							},
							_ => println!("Warning: Camera rotation must be a list of 3 numbers. Default will be used.")
						}

						match rotation[1] {
							json::Number(y) => {
								new_rotation.y = y as f32;
							},
							_ => println!("Warning: Camera rotation must be a list of 3 numbers. Default will be used.")
						}

						match rotation[2] {
							json::Number(z) => {
								new_rotation.z = z as f32;
							},
							_ => println!("Warning: Camera rotation must be a list of 3 numbers. Default will be used.")
						}
					} else {
						println!("Warning: Camera rotation must be a list of 3 numbers. Default will be used.");
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
		_ => {}
	}

	camera
}

fn materials_from_json(config: &json::Object) -> ~[~Material: Send+Freeze] {
	let default_material = ~materials::Diffuse{
		color: ~values::Number{value: 0.0} as ~ParametricValue: Send+Freeze
	} as ~Material: Send+Freeze;

	let mut materials = match config.find(&~"materials") {
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

	materials.push(default_material);
	return materials;
}

fn objects_from_json(config: &json::Object, material_count: uint) -> ~[~SceneObject: Send+Freeze] {
	match config.find(&~"objects") {
		Some(&json::List(ref objects)) => {
			objects.iter().filter_map(|o| {
				match o {
					&json::Object(ref object) => {
						shapes::from_json(object, material_count)
					},
					_ => None
				}
			}).collect()
		},
		_ => ~[]
	}
}