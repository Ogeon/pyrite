extern crate png;
extern crate extra;
extern crate sync;
extern crate nalgebra;
use std::cmp::{min, max};
use std::io::{File, stdio, Reader};
use std::io::BufferedReader;
use std::hashmap::HashMap;
use std::str::StrSlice;
use extra::time::precise_time_s;
use extra::json;
use nalgebra::na::Vec3;
use core::{Tracer, Camera, Scene, SceneObject, Material, ParametricValue, KdTree};
use wavefront::Mesh;
mod core;
mod shapes;
mod materials;
mod cameras;
mod values;
mod wavefront;

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
		match render(project, &project_dir) {
			Some(tracer) => {
				let response_curves = get_response_curves(project);

				tracer.pixels.access(|&ref mut values| {
					let (width, height) = tracer.image_size;
					save_png(project_dir.with_filename("render.png"), values, width, height, &response_curves);
				});
			},
			None => {}
		}
	} else {
		let mut stdin = BufferedReader::new(stdio::stdin());

		loop {
			print!("> ");
			stdio::flush();
			match stdin.read_line() {
				Ok(line) => {
					let args: ~[&str] = line.trim().splitn(' ', 1).collect();
					match args.as_slice() {
						[&"render"] => {
							match render(project, &project_dir) {
								Some(tracer) => {
									let response_curves = get_response_curves(project);

									tracer.pixels.access(|&ref mut values| {
										let (width, height) = tracer.image_size;
										save_png(project_dir.with_filename("render.png"), values, width, height, &response_curves);
									});
								},
								None => {}
							}
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
				_ => break
			}
		}
	}
}

fn render(project: &json::Object, path: &Path) -> Option<~Tracer> {
	match build_project(project, path) {
		Some(t) => {
			let mut tracer = ~t;
			tracer.bins = 40;

			let mut tracers = ~[];
			std::task::deschedule();
			for n in std::iter::range(0, 4) {
				println!("Starting render task {}", n);
				match tracer.spawn() {
					Some(task) => {
						tracers.push(task);
						std::task::deschedule();
					},
					None => break
				}
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

			Some(tracer)
		},
		None => None
	}
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

	match png::store_png(&image, &path) {
		Err(e) => println!("Error while saving image: {}", e),
		_ => {}
	}
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

fn load_project(path: &str) -> ~json::Object {
	let default = "{\"objects\": [], \"camera\": {}, \"materials\": [], \"render\": {}}";

	let mut project = if path.len() == 0 {
		//No file provided
		println!("New project created");
		json::from_str(default)
	} else {
		match File::open(&Path::new(path)) {
			//A valid path was provided
			Ok(mut file) => json::from_reader(&mut file as &mut Reader),

			//An invalid path was provided
			Err(e) => {
				println!("Unable to open {}: {}", path, e.desc);
				println!("New project created");
				json::from_str(default)
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

fn build_project(project: &json::Object, project_path: &Path) -> Option<Tracer> {
	let mut tracer = Tracer::new();

	match project.find(&~"render") {
		Some(&json::Object(ref render_cfg)) => {
			tracer_from_json(render_cfg, &mut tracer);
		},
		_ => println!("Warning: No valid render configurations provided")
	}

	match scene_from_json(project, project_path) {
		Some(scene) => {
			tracer.set_scene(scene);
			Some(tracer)
		},
		None => None
	}
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

fn scene_from_json(config: &json::Object, project_path: &Path) -> Option<Scene> {
	match camera_from_json(config) {
		Some(camera) => {
			let materials = materials_from_json(config);
			let objects = objects_from_json(config, materials.len() - 1, project_path);
			Some(Scene {
				camera: camera,
				objects: KdTree::build(objects),
				materials: materials
			})
		},
		None => None
	}
}

fn camera_from_json(config: &json::Object) -> Option<~Camera: Send+Freeze> {
	match config.find(&~"camera") {
		Some(&json::Object(ref camera_cfg)) => {
			cameras::from_json(camera_cfg)
		},
		_ => {
			println!("Error: No valid camera configuration provided");
			None
		}
	}
}

fn materials_from_json(config: &json::Object) -> ~[~Material: Send+Freeze] {
	let default_material = ~materials::Diffuse{
		color: ~values::Number{value: 1.0} as ~ParametricValue: Send+Freeze
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

fn objects_from_json(config: &json::Object, material_count: uint, project_path: &Path) -> ~[~SceneObject: Send+Freeze] {
	let mut models = HashMap::<~str, ~Mesh>::new();
	let mut meshes = ~[];

	let mut objects = match config.find(&~"objects") {
		Some(&json::List(ref objects)) => {
			objects.iter().filter_map(|o| {
				match o {
					&json::Object(ref object) => {
						match object.find(&~"type") {
							Some(&json::String(~"Mesh")) => {
								meshes.push(object);
								None
							},
							_ => shapes::from_json(object, material_count)
						}
						
					},
					_ => None
				}
			}).collect()
		},
		_ => ~[]
	};

	for ref config in meshes.iter() {
		let label = match config.find(&~"label") {
			Some(&json::String(ref label)) => label.to_owned(),
			_ => ~"<Mesh>"
		};


		match config.find(&~"file") {
			Some(&json::String(ref file_name)) => {
				let model = models.find_or_insert_with(file_name.to_owned(), |file| {
						wavefront::parse(&project_path.with_filename(file.to_owned()))
					}
				);

				let mut face_materials = HashMap::<u16, uint>::new();

				match config.find(&~"materials") {
					Some(&json::Object(ref material_config)) => {
						for (key, value) in material_config.iter() {
							match value {
								&json::Number(index) => {
									if (index as uint) < material_count {
										match model.get_group_index(key) {
											Some(group) => {
												face_materials.insert(group, index as uint);
											},
											_ => {}
										}
									} else {
										println!("Warning: Unknown material indiex {} for group {} in mesh \"{}\". Default will be used.", index as uint, key.to_str(), label);
									}
								},
								_ => println!("Warning: material indices for mesh \"{}\" must be numbers.", label)
							}
						}
					},
					None => println!("Warning: \"materials\" for mesh \"{}\" must be an object with material indices. Default will be used.", label),
					_ => println!("Warning: \"materials\" for mesh \"{}\" is not set. Default will be used.", label)
				}

				for indices in model.indices.chunks(3) {
					let v1 = model.vertices[indices[0]];
					let v2 = model.vertices[indices[1]];
					let v3 = model.vertices[indices[2]];

					let material = match face_materials.find_copy(&v1.group) {
						Some(index) => {
							index
						},
						None => material_count
					};

					objects.push(
						~shapes::Triangle::new(
							Vec3::new(v1.position[0], v1.position[1], v1.position[2]),
							Vec3::new(v2.position[0], v2.position[1], v2.position[2]),
							Vec3::new(v3.position[0], v3.position[1], v3.position[2]),
							material
						) as ~SceneObject: Send+Freeze
					);
				}
			},
			None => {
				println!("Warning: missing \"file\" for the mesh \"{}\". The mesh will not be used.", label);
				continue;
			},
			_ => {
				println!("Warning: \"file\" for the mesh \"{}\" must be a string. The mesh will not be used.", label);
				continue;
			}
		}
	}

	objects
}