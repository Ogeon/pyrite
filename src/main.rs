extern crate cgmath;
extern crate image;
extern crate obj;
extern crate genmesh;
extern crate rand;
extern crate threadpool;
extern crate num_cpus;

use std::sync::{Arc, RwLock};
use std::fs::File;
use std::path::Path;

use threadpool::ThreadPool;

use cgmath::Vector2;

use image::GenericImage;

use renderer::Tile;

macro_rules! try {
    ($e:expr) => (
        match $e {
            Ok(v) => v,
            Err(e) => return Err(e)
        }
    );

    ($e:expr, $under:expr) => (
        match $e {
            Ok(v) => v,
            Err(e) => return Err(format!("{}: {}", $under, e))
        }
    )
}

mod tracer;
mod bkdtree;
mod cameras;
mod shapes;
mod materials;
mod config;
mod project;
mod renderer;
mod types3d;
mod math;
mod values;

fn main() {
    let mut args = std::env::args();
    let name = args.next().unwrap_or("pyrite".into());

    if let Some(project_path) = args.next() {
        match project::from_file(&project_path) {
            project::ParseResult::Success(p) => render(p, project_path),
            project::ParseResult::IoError(e) => println!("error while reading project file: {}", e),
            project::ParseResult::ParseError(e) => println!("error while parsing project file: {}", e)
        }
    } else {
        println!("usage: {} project_file", name);
    }
}

fn render<P: AsRef<Path>>(project: project::Project, project_path: P) {
    let image_size = Vector2::new(project.image.width, project.image.height);

    let tiles = project.renderer.make_tiles(&project.camera, &image_size);
    let tile_count = tiles.len();

    let config = Arc::new(RenderContext {
        camera: project.camera,
        world: project.world,
        pending: RwLock::new(tiles),
        completed: RwLock::new(Vec::new()),
        renderer: project.renderer
    });

    let pool = ThreadPool::new(config.renderer.threads);

    for _ in 0..tile_count {
        let context = config.clone();
        pool.execute(move || {
            let mut tile = {
                context.pending.write().unwrap().pop().unwrap()
            };
            println!("Task rendering tile {:?}", tile.screen_area().from);

            context.renderer.render_tile(&mut tile, &context.camera, &context.world);

            context.completed.write().unwrap().push(tile);
        })
    }

    let mut tile_counter = 0;

    let mut pixels = image::ImageBuffer::new(image_size.x as u32, image_size.y as u32);

    let (red, green, blue) = project.image.rgb_curves;

    let red = math::utils::Interpolated {
        points: red
    };

    let green = math::utils::Interpolated {
        points: green
    };

    let blue = math::utils::Interpolated {
        points: blue
    };

    let project_path = project_path.as_ref();
    let render_path = project_path.parent().unwrap_or(project_path).join("render.png");
    
    while tile_counter < tile_count {
        std::thread::sleep_ms(4000);

        loop {
            match config.completed.write().unwrap().pop() {
                Some(tile) => {
                    for (spectrum, position) in tile.pixels() {
                        let r = clamp_channel(calculate_channel(&spectrum, &red));
                        let g = clamp_channel(calculate_channel(&spectrum, &green));
                        let b = clamp_channel(calculate_channel(&spectrum, &blue));
                        
                        pixels.put_pixel(position.x as u32, position.y as u32, image::Rgb {
                            data: [r, g, b]
                        })
                    }

                    tile_counter += 1;
                },
                None => break
            }
        }

        match File::create(&render_path) {
            Ok(mut file) => if let Err(e) = image::ImageRgb8(pixels.clone()).save(&mut file, image::PNG) {
                println!("error while writing image: {}", e);
            },
            Err(e) => println!("failed to open/create file for writing: {}", e)
        }
    }

    println!("Done!")
}

fn calculate_channel(spectrum: &renderer::Spectrum, response: &math::utils::Interpolated) -> f64 {
    let mut sum = 0.0;
    let mut weight = 0.0;

    for segment in spectrum.segments() {
        let mut offset = 0.0;
        let mut start_resp = response.get(segment.start);

        while offset < segment.width {
            let step = (segment.width - offset).min(5.0);
            let end_resp = response.get(segment.start + offset + step);

            let w = (start_resp + end_resp) * step;
            sum += segment.value * w;
            weight += w;

            start_resp = end_resp;
            offset += step;
        }
        
    }

    (sum / weight).powf(0.45)
}

struct RenderContext {
    camera: cameras::Camera,
    world: tracer::World,
    pending: RwLock<Vec<Tile>>,
    completed: RwLock<Vec<Tile>>,
    renderer: renderer::Renderer
}

fn clamp_channel(value: f64) -> u8 {
    if value >= 1.0 {
        255
    } else if value <= 0.0 {
        0
    } else {
        (value * 255.0) as u8
    }
}