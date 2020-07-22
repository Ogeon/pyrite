#![cfg_attr(test, allow(dead_code))]

use std::time::Instant;

use image;

use std::{
    error::Error,
    io::{stdout, Write},
    ops::{Add, AddAssign, Div, Mul},
    path::Path,
};

use cgmath::Vector2;

use palette::{ComponentWise, FromColor, LinSrgb, Pixel, Srgb, Xyz};

use bumpalo::Bump;

use color::{Light, WavelengthInput};
use film::{Film, Spectrum};
use project::{
    eval_context::EvalContext,
    expressions::Expressions,
    meshes::Meshes,
    program::{ExecutionContext, Program, ProgramCompiler, ProgramInput, Resources},
    ProjectData,
};

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
    let arena = Bump::new();

    if let Some(project_path) = args.next() {
        let ProjectData {
            expressions,
            meshes,
            spectra,
            textures,
            project,
        } = match project::load_project(&project_path) {
            Ok(project) => project,
            Err(error) => {
                eprintln!("error while loading project file: {}", error);
                return;
            }
        };

        let programs = ProgramCompiler::new(&arena);
        let resources = Resources {
            spectra: &spectra,
            textures: &textures,
        };

        match parse_project(project, programs, &expressions, &meshes, resources, &arena) {
            Ok((image, context)) => render(image, context, project_path),
            Err(error) => eprintln!("error while parsing project: {}", error),
        };
    } else {
        eprintln!("usage: {} project_file", name);
    }
}

fn parse_project<'p>(
    project: project::Project,
    programs: ProgramCompiler<'p>,
    expressions: &Expressions,
    meshes: &Meshes,
    resources: Resources<'p>,
    arena: &'p Bump,
) -> Result<(ImageSettings<'p>, RenderContext<'p>), Box<dyn Error>> {
    let eval_context = EvalContext { expressions };

    let config = RenderContext {
        camera: cameras::Camera::from_project(project.camera, eval_context)?,
        renderer: renderer::Renderer::from_project(project.renderer),
        world: world::World::from_project(
            project.world,
            eval_context,
            programs,
            expressions,
            meshes,
            &arena,
        )?,
        resources,
    };

    let image = ImageSettings::from_project(project.image, programs, expressions)?;

    Ok((image, config))
}

fn render<P: AsRef<Path>>(
    image_settings: ImageSettings<'_>,
    config: RenderContext<'_>,
    project_path: P,
) {
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

    let mut filter_exe = ExecutionContext::new(config.resources);
    let mut filter = image_settings.filter.map(|white| {
        move |intensity: f32, wavelength: f32| {
            intensity
                * filter_exe
                    .run(white, &SpectrumSamplingInput { wavelength })
                    .value
        }
    });

    let mut white_balance_exe = ExecutionContext::new(config.resources);
    let mut white_balance = image_settings.white.map(|white| {
        let mut wavelength = config.renderer.spectrum_span.0;
        let mut max = 0.0f32;
        let mut d65_max = 0.0f32;

        while wavelength < config.renderer.spectrum_span.1 {
            max = max.max(
                white_balance_exe
                    .run(white, &SpectrumSamplingInput { wavelength })
                    .value,
            );
            d65_max = d65_max.max(light_source::D65.get(wavelength));
            wavelength += 1.0;
        }

        move |intensity: f32, wavelength: f32| {
            let white_intensity = white_balance_exe
                .run(white, &SpectrumSamplingInput { wavelength })
                .value
                / max;
            let neutral = intensity / white_intensity.max(0.000001);
            neutral * (light_source::D65.get(wavelength) / d65_max)
        }
    });

    let mut spectrum_get = |spectrum: &Spectrum, wavelength: f32| {
        let intensity = spectrum.get(wavelength);

        let filtered = if let Some(filter) = &mut filter {
            filter(intensity, wavelength)
        } else {
            intensity
        };

        if let Some(white_balance) = &mut white_balance {
            white_balance(filtered, wavelength)
        } else {
            filtered
        }
    };

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
                            let color = spectrum_to_xyz(
                                spectrum.spectrum_width(),
                                30.0,
                                spectrum,
                                |s, w| spectrum_get(s, w),
                            );
                            Srgb::from_color(color).into_format()
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
        config.resources,
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
            let color = spectrum_to_xyz(spectrum.spectrum_width(), 2.0, spectrum, |s, w| {
                spectrum_get(s, w)
            });
            Srgb::from_color(color).into_format()
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
    red: &project::spectra::Spectrum,
    green: &project::spectra::Spectrum,
    blue: &project::spectra::Spectrum,
) -> LinSrgb {
    spectrum_to_tristimulus(
        spectrum.spectrum_width(),
        step_size,
        spectrum,
        Spectrum::get,
        red,
        green,
        blue,
    )
}

fn spectrum_to_xyz<S>(
    spectrum_width: (f32, f32),
    step_size: f32,
    spectrum: S,
    sample: impl FnMut(&S, f32) -> f32,
) -> Xyz {
    let color: Xyz = spectrum_to_tristimulus(
        spectrum_width,
        step_size,
        spectrum,
        sample,
        &xyz::response::X,
        &xyz::response::Y,
        &xyz::response::Z,
    );

    color * 3.444 // Scale up to better match D65 light source data
}

fn spectrum_to_tristimulus<T, S>(
    (min, max): (f32, f32),
    step_size: f32,
    spectrum: S,
    mut sample: impl FnMut(&S, f32) -> f32,
    first: &project::spectra::Spectrum,
    second: &project::spectra::Spectrum,
    third: &project::spectra::Spectrum,
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
{
    let mut sum = T::from((0.0, 0.0, 0.0));
    let mut weight = 0.0;

    let mut wl_min = min;
    let mut spectrum_min = sample(&spectrum, wl_min);

    while wl_min < max {
        let wl_max = wl_min + step_size;

        let spectrum_max = sample(&spectrum, wl_max);
        let (first_min, first_max) = (first.get(wl_min), first.get(wl_max));
        let (second_min, second_max) = (second.get(wl_min), second.get(wl_max));
        let (third_min, third_max) = (third.get(wl_min), third.get(wl_max));

        let start_resp = T::from((first_min, second_min, third_min));
        let end_resp = T::from((first_max, second_max, third_max));

        let w = wl_max - wl_min;
        sum += (start_resp * spectrum_min + end_resp * spectrum_max) * 0.5 * w;
        weight += w;

        wl_min = wl_max;
        spectrum_min = spectrum_max;
    }

    if weight == 0.0 {
        sum
    } else {
        sum / weight
    }
}

struct RenderContext<'p> {
    camera: cameras::Camera,
    world: world::World<'p>,
    renderer: renderer::Renderer,
    resources: Resources<'p>,
}

struct ImageSettings<'a> {
    width: u32,
    height: u32,
    file: Option<String>,
    filter: Option<Program<'a, SpectrumSamplingInput, Light>>,
    white: Option<Program<'a, SpectrumSamplingInput, Light>>,
}

impl<'a> ImageSettings<'a> {
    fn from_project(
        project: project::Image,
        programs: ProgramCompiler<'a>,
        expressions: &Expressions,
    ) -> Result<Self, Box<dyn Error>> {
        let project::Image {
            width,
            height,
            file,
            filter,
            white,
        } = project;

        Ok(ImageSettings {
            width,
            height,
            file,
            filter: filter
                .map(|filter| programs.compile(&filter, expressions))
                .transpose()?,
            white: white
                .map(|white| programs.compile(&white, expressions))
                .transpose()?,
        })
    }
}

struct SpectrumSamplingInput {
    wavelength: f32,
}

impl ProgramInput for SpectrumSamplingInput {
    fn normal() -> Result<project::program::InputFn<Self>, Box<dyn Error>> {
        Err("the surface normal cannot be used while sampling a constant spectrum".into())
    }
    fn incident() -> Result<project::program::InputFn<Self>, Box<dyn Error>> {
        Err("the incident vector cannot be used while sampling a constant spectrum".into())
    }
    fn texture_coordinates() -> Result<project::program::InputFn<Self>, Box<dyn Error>> {
        Err("texture coordinates cannot be used while sampling a constant spectrum".into())
    }
}

impl WavelengthInput for SpectrumSamplingInput {
    fn wavelength(&self) -> f32 {
        self.wavelength
    }
}
