use std::io::{BufferedReader, File};
use std::hashmap::HashMap;
use std::num;
use std::from_str;

pub struct Vertex {
	position: [f32, ..3],
	normal: [f32, ..3],
	texture: [f32, ..2],
	group: u16
}

pub struct Mesh {
	vertices: ~[Vertex],
	indices: ~[u16],
	groups: ~HashMap<~str, u16>
}


impl Mesh {
	pub fn get_vertex_buffer(&self) -> ~[f32] {
		let mut buffer : ~[f32] = ~[];
		for &vertex in self.vertices.iter() {
			for &value in vertex.position.iter() {
				buffer.push(value);
			}
			for &value in vertex.normal.iter() {
				buffer.push(value);
			}
			for &value in vertex.texture.iter() {
				buffer.push(value);
			}
		}

		buffer
	}

	pub fn get_triangle_buffer(&self) -> ~[f32] {
		let mut buffer : ~[f32] = ~[];
		for &index in self.indices.iter() {
			let vertex = &self.vertices[index];

			for &value in vertex.position.iter() {
				buffer.push(value);
			}
			for &value in vertex.normal.iter() {
				buffer.push(value);
			}
			for &value in vertex.texture.iter() {
				buffer.push(value);
			}
		}

		buffer
	}

	pub fn get_group_index(&self, name : &~str) -> Option<u16> {
		self.groups.find_copy(name)
	}
}


pub fn parse(filename: &Path) -> ~Mesh {
	let mut vertices: ~[Vertex] = ~[];
	let mut faces: ~[~[~[uint, ..4]]] = ~[];
	let mut indices: ~[u16] = ~[];
	let mut points: ~[~[f32]] = ~[];
	let mut tex_coords: ~[~[f32]] = ~[];
	let mut normals: ~[~[f32]] = ~[];
	let mut groups = ~HashMap::<~str, u16>::new();
	let mut vertex_map = ~HashMap::<~str, uint>::new();

	let file = File::open(filename);
	let mut reader = BufferedReader::new(file);

	let mut current_group = 0u16;
	let mut last_group = current_group;

	//Read file
	loop {
		match reader.read_line() {
			Some(line) => {
				let parts: ~[~str] = line.replace("  ", " ").split(' ').map(|part| {
					part.to_owned()
				}).collect();

				match &parts[0] {
					&~"f" => faces.push(parse_face(parts, current_group as uint)),
					&~"v" => points.push(parse_3f(parts)),
					&~"vn" => normals.push(parse_3f(parts)),
					&~"vt" => tex_coords.push(parse_2f(parts)),
					&~"g" => {
							let name = parts[1].trim();
							if groups.contains_key(&name.to_owned()) {
								//println!("Found old group {}", name);
								current_group = groups.get(&name.to_owned())+1;
							} else {
								//println!("Found new group {}", name);
								last_group += 1;
								current_group = last_group;
								groups.insert(name.to_owned(), current_group-1);
							}
						},
					_ => continue
				}
			},
			_ => break
		}
	}

	let no_normals = normals.len() == 0;

	//Make missing normals
	for face in faces.mut_iter() {
		if face[0][2] == 0 || face[1][2] == 0 || face[2][2] == 0 || no_normals {
			face[0][2] = normals.len() + 1;
			face[1][2] = normals.len() + 1;
			face[2][2] = normals.len() + 1;

			let u = sub(points[face[1][0]-1], points[face[0][0]-1]);
			let v = sub(points[face[2][0]-1], points[face[0][0]-1]);

			normals.push(normalize(cross(u, v)));
		}
	}

	//Fallback texture coordinate
	tex_coords.push(~[0f32, 0f32]);

	//Fallback group
	last_group += 1;
	groups.insert(~"no group", last_group-1);

	//Make vertices
	for face in faces.iter() {
		for vertex in face.iter() {
			let v = vertex[0];
			let mut t = vertex[1];
			let n = vertex[2];
			let mut g = vertex[3] as u16;

			if t == 0 || t > tex_coords.len() {
				t = tex_coords.len();
			}

			if g == 0 {
				g = last_group;
			}

			let vertex_name = format!("{}/{}/{}/{}", v, t, n, g as uint);

			if vertex_map.contains_key(&vertex_name) {
				indices.push(vertex_map.get_copy(&vertex_name) as u16);
			} else {
				let index = vertices.len();
				vertex_map.insert(vertex_name, index);
				let pos = &points[v-1];
				let nor = &normals[n-1];
				let tex = &tex_coords[t-1];

				vertices.push(Vertex{
					position: [pos[0], pos[1], pos[2]],
					normal: [nor[0], nor[1], nor[2]],
					texture: [tex[0], tex[1]],
					group: (g-1) as u16
				});

				indices.push(index as u16);
			}
		}
	}
	
//	println!(format!("Faces: {}, Points: {}, Normals: {}, Texture coordinates: {}, Groups: {}",
//		faces.len(), points.len(), normals.len(), tex_coords.len(), (last_group-1) as uint));

	~Mesh {indices: indices, vertices: vertices, groups: groups}
}

fn parse_face(line: &[~str], group: uint) -> ~[~[uint, ..4]] {
	//println!("Parsing line {}", line.to_str());

	line.iter().skip(1).map(|point| {
		~parse_face_point(point.to_owned().trim().split('/').collect(), group)
	}).collect()
}

fn parse_face_point(point: ~[&str], group: uint) ->  [uint, ..4] {
	if point.len() > 1 {
		[
			from_str(point[0].to_owned()).unwrap_or(0),
			from_str(point[1].to_owned()).unwrap_or(0),
			from_str(point[2].to_owned()).unwrap_or(0),
			group
		]
	} else {
		let i = from_str(point[0]).unwrap_or(0);
		[i, i, i, group]
	}
}

fn parse_3f(line: &[~str]) -> ~[f32] {
	if line.len() < 4 {
		~[
			from_str(line[1]).unwrap_or(0f32),
			from_str(line[2]).unwrap_or(0f32),
			0f32
		]
	} else {
		~[
			from_str(line[1]).unwrap_or(0f32),
			from_str(line[2]).unwrap_or(0f32),
			from_str(line[3].trim()).unwrap_or(0f32)
		]
	}
}

fn parse_2f(line: &[~str]) -> ~[f32] {
	~[
		from_str(line[1]).unwrap_or(0.0),
		from_str(line[2]).unwrap_or(0.0)
	]
}

fn cross(u : &[f32], v : &[f32]) -> ~[f32] {
	~[
		u[1]*v[2] - u[2]*v[1],
		u[2]*v[0] - u[0]*v[2],
		u[0]*v[1] - u[1]*v[0]
	]
}

fn normalize(v :  &[f32]) -> ~[f32] {
	let size = num::sqrt(v[0]*v[0] + v[1]*v[1] + v[2]*v[2]);
	if size != 0f32 {
		~[v[0]/size, v[1]/size, v[2]/size]
	} else {
		~[v[0], v[1], v[2]]
	}
}

fn sub(u : &[f32], v : &[f32]) -> ~[f32] {
	~[u[0]-v[0], u[1]-v[1], u[2]-v[2]]
}