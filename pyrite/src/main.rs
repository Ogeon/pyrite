#![cfg_attr(test, allow(dead_code))]

extern crate cgmath;
extern crate collision;
extern crate crossbeam;
extern crate genmesh;
extern crate image;
extern crate num_cpus;
extern crate obj;
extern crate palette;
extern crate pyrite_config as config;
extern crate rand;
extern crate rand_xorshift;
extern crate rayon;
extern crate time;

use std::fs::File;
use std::io::{stdout, Write};
use std::path::Path;

use cgmath::Vector2;

use image::GenericImage;

use time::PreciseTime;

use rand::Rng;
use rand_xorshift::XorShiftRng;

use palette::{LinSrgb, Pixel, Srgb};

use film::{Film, Spectrum, Tile};

macro_rules! try {
    ($e:expr) => {
        match $e {
            Ok(v) => v,
            Err(e) => return Err(e),
        }
    };

    ($e:expr, $under:expr) => {
        match $e {
            Ok(v) => v,
            Err(e) => return Err(format!("{}: {}", $under, e)),
        }
    };
}

mod cameras;
mod film;
mod lamp;
mod materials;
mod math;
mod project;
mod renderer;
mod shapes;
mod spatial;
mod tracer;
mod types3d;
mod utils;
mod values;
mod world;

fn main() {
    let mut args = std::env::args();
    let name = args.next().unwrap_or("pyrite".into());

    if let Some(project_path) = args.next() {
        match project::from_file(&project_path) {
            project::ParseResult::Success(p) => render(p, project_path),
            project::ParseResult::ParseError(e) => {
                println!("error while parsing project file: {}", e)
            }
            project::ParseResult::InterpretError(e) => {
                println!("error while interpreting project file: {}", e)
            }
        }
    } else {
        println!("usage: {} project_file", name);
    }
}

fn render<P: AsRef<Path>>(project: project::Project<XorShiftRng>, project_path: P) {
    let image_size = Vector2::new(project.image.width, project.image.height);

    let config = RenderContext {
        camera: project.camera,
        world: project.world,
        renderer: project.renderer,
    };

    let mut pool = renderer::RayonPool;

    let mut pixels = image::ImageBuffer::new(image_size.x as u32, image_size.y as u32);

    let (red, green, blue) = project.image.rgb_curves;

    let red = math::utils::Interpolated { points: red };

    let green = math::utils::Interpolated { points: green };

    let blue = math::utils::Interpolated { points: blue };

    let project_path = project_path.as_ref();
    let render_path = project_path
        .parent()
        .unwrap_or(project_path)
        .join("render.png");

    /*let f = |mut tile: Tile| {
        config.renderer.render_tile(&mut tile, &config.camera, &config.world);
    };*/

    let film = Film::new(
        image_size.x,
        image_size.y,
        config.renderer.tile_size,
        config.renderer.spectrum_span,
        config.renderer.spectrum_bins,
        &config.camera,
    );

    let mut last_print = PreciseTime::now();

    config.renderer.render(
        &film,
        &mut pool,
        |status| {
            if last_print.to(PreciseTime::now()).num_milliseconds() >= 500 {
                print!("\r{}... {:2}%", status.message, status.progress);
                stdout().flush().unwrap();

                if last_print.to(PreciseTime::now()).num_seconds() >= 4 {
                    let begin_iter = PreciseTime::now();
                    film.with_changed_pixels(|position, spectrum| {
                        let r = calculate_channel(&spectrum, &red);
                        let g = calculate_channel(&spectrum, &green);
                        let b = calculate_channel(&spectrum, &blue);

                        let rgb: Srgb<u8> = Srgb::from_linear(LinSrgb::new(r, g, b)).into_format();

                        unsafe {
                            pixels.unsafe_put_pixel(
                                position.x as u32,
                                position.y as u32,
                                image::Rgb(rgb.into_raw()),
                            )
                        }
                    });
                    let diff = begin_iter.to(PreciseTime::now()).num_milliseconds() as f64 / 1000.0;

                    print!(
                        "\r{}... {:2}% - updated image in {} seconds",
                        status.message, status.progress, diff
                    );
                    stdout().flush().unwrap();
                    if let Err(e) = pixels.save(&render_path) {
                        println!("\rerror while writing image: {}", e);
                    }
                    last_print = PreciseTime::now();
                }
            }
        },
        &config.camera,
        &config.world,
    );

    /*crossbeam::scope(|scope| {
        print!(" 0%");
        stdout().flush().unwrap();

        let mut last_print = PreciseTime::now();
        let num_tiles = film.num_tiles();

        for (i, _) in pool.unordered_map(scope, &film, &f).enumerate() {
            print!("\r{:2}%", (i * 100) / num_tiles);
            stdout().flush().unwrap();
            if last_print.to(PreciseTime::now()).num_seconds() >= 4 {
                let begin_iter = PreciseTime::now();
                film.with_changed_pixels(|position, spectrum| {
                    let r = clamp_channel(calculate_channel(&spectrum, &red));
                    let g = clamp_channel(calculate_channel(&spectrum, &green));
                    let b = clamp_channel(calculate_channel(&spectrum, &blue));

                    unsafe {
                        pixels.unsafe_put_pixel(position.x as u32, position.y as u32, image::Rgb {
                            data: [r, g, b]
                        })
                    }
                });
                let diff = begin_iter.to(PreciseTime::now()).num_milliseconds() as f64 / 1000.0;

                print!("\r{:2}% - updated iamge in {} seconds", (i * 100) / num_tiles, diff);
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
    });*/

    film.with_changed_pixels(|position, spectrum| {
        let r = calculate_channel(&spectrum, &red);
        let g = calculate_channel(&spectrum, &green);
        let b = calculate_channel(&spectrum, &blue);

        let rgb = Srgb::from_linear(LinSrgb::new(r, g, b)).into_format();

        unsafe {
            pixels.unsafe_put_pixel(
                position.x as u32,
                position.y as u32,
                image::Rgb(rgb.into_raw()),
            )
        }
    });

    if let Err(e) = pixels.save(&render_path) {
        println!("\rerror while writing image: {}", e);
    }

    println!("\rDone!")
}

fn calculate_channel(spectrum: &Spectrum, response: &math::utils::Interpolated) -> f64 {
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

    sum / weight
}

struct RenderContext<R: Rng> {
    camera: cameras::Camera,
    world: world::World<R>,
    renderer: renderer::Renderer,
}
