#![cfg_attr(test, allow(dead_code))]

extern crate cgmath;
extern crate image;
extern crate obj;
extern crate genmesh;
extern crate rand;
extern crate simple_parallel;
extern crate num_cpus;
extern crate time;
extern crate pyrite_config as config;

use std::fs::File;
use std::path::Path;
use std::io::{Write, stdout};

use simple_parallel::Pool;

use cgmath::Vector2;

use image::GenericImage;

use time::PreciseTime;

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
            project::ParseResult::ParseError(e) => println!("error while parsing project file: {}", e),
            project::ParseResult::InterpretError(e) => println!("error while interpreting project file: {}", e),
        }
    } else {
        println!("usage: {} project_file", name);
    }
}

fn render<P: AsRef<Path>>(project: project::Project, project_path: P) {
    let image_size = Vector2::new(project.image.width, project.image.height);

    let tiles = project.renderer.make_tiles(&project.camera, &image_size);

    let config = RenderContext {
        camera: project.camera,
        world: project.world,
        renderer: project.renderer
    };

    let mut pool = Pool::new(config.renderer.threads);

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

    let f = |mut tile: Tile| {
        config.renderer.render_tile(&mut tile, &config.camera, &config.world);
        tile
    };

    print!(" 0%");
    stdout().flush().unwrap();

    let mut last_print = PreciseTime::now();
    let num_tiles = tiles.len();
    for (i, (_, tile)) in unsafe { pool.unordered_map(tiles, &f) }.enumerate() {
        for (spectrum, position) in tile.pixels() {
            let r = clamp_channel(calculate_channel(&spectrum, &red));
            let g = clamp_channel(calculate_channel(&spectrum, &green));
            let b = clamp_channel(calculate_channel(&spectrum, &blue));
            
            pixels.put_pixel(position.x as u32, position.y as u32, image::Rgb {
                data: [r, g, b]
            })
        }

        if last_print.to(PreciseTime::now()).num_seconds() >= 4 {
            print!("\r{:2}%", (i * 100) / num_tiles);
            stdout().flush().unwrap();
            match File::create(&render_path) {
                Ok(mut file) => if let Err(e) = image::ImageRgb8(pixels.clone()).save(&mut file, image::PNG) {
                    println!("\rerror while writing image: {}", e);
                },
                Err(e) => println!("\rfailed to open/create file for writing: {}", e)
            }
            last_print = PreciseTime::now();
        }
    }

    match File::create(&render_path) {
        Ok(mut file) => if let Err(e) = image::ImageRgb8(pixels.clone()).save(&mut file, image::PNG) {
            println!("\rerror while writing image: {}", e);
        },
        Err(e) => println!("\rfailed to open/create file for writing: {}", e)
    }

    println!("\rDone!")
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