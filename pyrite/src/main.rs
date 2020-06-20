#![cfg_attr(test, allow(dead_code))]

use std::time::Instant;

use image;

use std::io::{stdout, Write};
use std::{
    error::Error,
    ops::{Add, AddAssign, Div, Mul},
    path::{Path, PathBuf},
};

use cgmath::Vector2;

use palette::{ComponentWise, LinSrgb, Pixel, Srgb, Xyz};

use crate::film::{Film, Spectrum};
use crate::math::utils::Interpolated;

mod cameras;
mod color;
mod film;
mod lamp;
mod light_source;
mod materials;
mod math;
mod project;
mod renderer;
mod rgb;
mod shapes;
mod spatial;
mod texture;
mod tracer;
mod utils;
mod world;
mod xyz;

fn main() {
    let mut args = std::env::args();
    let name = args.next().unwrap_or("pyrite".into());

    if let Some(project_path) = args.next() {
        let project = match project::load_project(&project_path) {
            Ok(project) => project,
            Err(error) => {
                eprintln!("error while loading project file: {}", error);
                return;
            }
        };

        let project_dir = Path::new(&project_path)
            .parent()
            .expect("could not get the project path parent directory");

        match parse_project(project, |path| project_dir.join(path)) {
            Ok((image, context)) => render(image, context, project_path),
            Err(error) => eprintln!("error while parsing project: {}", error),
        }
    } else {
        eprintln!("usage: {} project_file", name);
    }
}

fn parse_project(
    project: project::Project,
    make_path: impl Fn(&str) -> PathBuf,
) -> Result<(project::Image, RenderContext), Box<dyn Error>> {
    let config = RenderContext {
        camera: cameras::Camera::from_project(project.camera, &make_path)?,
        renderer: renderer::Renderer::from_project(project.renderer, &make_path)?,
        world: world::World::from_project(project.world, &make_path)?,
    };

    Ok((project.image, config))
}

fn render<P: AsRef<Path>>(image_settings: project::Image, config: RenderContext, project_path: P) {
    let image_size = Vector2::new(image_settings.width, image_settings.height);

    let mut pool = renderer::RayonPool;

    let mut pixels = image::ImageBuffer::new(image_size.x, image_size.y);

    let rgb_curves = None; /*image_settings.rgb_curves.map(|(red, green, blue)| {
                               (
                                   Interpolated { points: red },
                                   Interpolated { points: green },
                                   Interpolated { points: blue },
                               )
                           });*/

    let project_path = project_path.as_ref();
    let render_path = project_path
        .parent()
        .unwrap_or(project_path)
        .join("render.png");

    /*let f = |mut tile: Tile| {
        config.renderer.render_tile(&mut tile, &config.camera, &config.world);
    };*/

    let film = Film::new(
        image_size.x as usize,
        image_size.y as usize,
        config.renderer.spectrum_bins,
        config.renderer.spectrum_span,
    );

    let mut last_print: Option<Instant> = None;
    let mut last_image: Instant = Instant::now();

    config.renderer.render(
        &film,
        &mut pool,
        |status| {
            let time_since_print = last_print.map(|last_print| Instant::now() - last_print);

            let should_print = time_since_print
                .map(|time| time.as_millis() >= 500)
                .unwrap_or(true);

            if should_print {
                print!("\r{}... {:2}%", status.message, status.progress);
                stdout().flush().unwrap();
                last_print = Some(Instant::now());

                let time_since_image = Instant::now() - last_image;
                if time_since_image.as_secs() >= 20 {
                    let begin_iter = Instant::now();
                    for (spectrum, pixel) in film.developed_pixels().zip(pixels.pixels_mut()) {
                        let rgb: Srgb<u8> = if let Some((red, green, blue)) = &rgb_curves {
                            let color = spectrum_to_rgb(30.0, spectrum, &red, &green, &blue);
                            Srgb::from_linear(color).into_format()
                        } else {
                            let color = spectrum_to_xyz(30.0, spectrum);
                            Srgb::from(color).into_format()
                        };

                        *pixel = image::Rgb(rgb.into_raw());
                    }
                    let diff = (Instant::now() - begin_iter).as_millis() as f64 / 1000.0;

                    print!(
                        "\r{}... {:2}% - updated image in {} seconds",
                        status.message, status.progress, diff
                    );
                    stdout().flush().unwrap();
                    if let Err(e) = pixels.save(&render_path) {
                        println!("\rerror while writing image: {}", e);
                    }
                    last_image = Instant::now();
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

    println!("\nSaving final result...");

    for (spectrum, pixel) in film.developed_pixels().zip(pixels.pixels_mut()) {
        let rgb: Srgb<u8> = if let Some((red, green, blue)) = &rgb_curves {
            let color = spectrum_to_rgb(2.0, spectrum, &red, &green, &blue);
            Srgb::from_linear(color).into_format()
        } else {
            let color = spectrum_to_xyz(2.0, spectrum);
            Srgb::from(color).into_format()
        };

        *pixel = image::Rgb(rgb.into_raw());
    }

    if let Err(e) = pixels.save(&render_path) {
        println!("error while writing image: {}", e);
    }

    println!("Done!")
}

fn spectrum_to_rgb(
    step_size: f32,
    spectrum: Spectrum,
    red: &Interpolated,
    green: &Interpolated,
    blue: &Interpolated,
) -> LinSrgb {
    spectrum_to_tristimulus(step_size, spectrum, red, green, blue)
}

fn spectrum_to_xyz(step_size: f32, spectrum: Spectrum) -> Xyz {
    let color: Xyz = spectrum_to_tristimulus(
        step_size,
        spectrum,
        &Interpolated {
            points: xyz::response::X,
        },
        &Interpolated {
            points: xyz::response::Y,
        },
        &Interpolated {
            points: xyz::response::Z,
        },
    );

    color * 3.444 // Scale up to better match D65 light source data
}

fn spectrum_to_tristimulus<T, P>(
    step_size: f32,
    spectrum: Spectrum,
    first: &Interpolated<P>,
    second: &Interpolated<P>,
    third: &Interpolated<P>,
) -> T
where
    T: ComponentWise<Scalar = f32>
        + From<(f32, f32, f32)>
        + Into<(f32, f32, f32)>
        + Add<Output = T>
        + Mul<Output = T>
        + Mul<f32, Output = T>
        + Div<f32, Output = T>
        + AddAssign
        + Copy,
    P: AsRef<[(f32, f32)]>,
{
    let mut sum = T::from((0.0, 0.0, 0.0));
    let mut weight = 0.0;

    let (min, max) = spectrum.spectrum_width();
    let num_segments = ((max - min) / step_size).ceil() as usize;
    let segments = spectrum
        .segments_between(min, max, num_segments)
        .zip(first.segments_between(min, max, num_segments))
        .zip(second.segments_between(min, max, num_segments))
        .zip(third.segments_between(min, max, num_segments));

    for (((spectrum, first), second), third) in segments {
        let ((wl_min, spectrum_min), (wl_max, spectrum_max)) = spectrum;
        let ((_, first_min), (_, first_max)) = first;
        let ((_, second_min), (_, second_max)) = second;
        let ((_, third_min), (_, third_max)) = third;

        let start_resp = T::from((first_min, second_min, third_min));
        let end_resp = T::from((first_max, second_max, third_max));

        let w = wl_max - wl_min;
        sum += (start_resp * spectrum_min + end_resp * spectrum_max) * 0.5 * w;
        weight += w;
    }

    if weight == 0.0 {
        sum
    } else {
        sum / weight
    }
}

struct RenderContext {
    camera: cameras::Camera,
    world: world::World,
    renderer: renderer::Renderer,
}
