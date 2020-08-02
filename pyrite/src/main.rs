#![cfg_attr(test, allow(dead_code))]

use std::time::Instant;

use image;

use std::{
    borrow::Cow,
    convert::TryFrom,
    error::Error,
    ops::{Add, AddAssign, Div, Mul},
    path::Path,
};

use cgmath::Vector2;

use palette::{ComponentWise, FromColor, LinSrgb, Pixel, Srgb, Xyz};

use bumpalo::Bump;

use indicatif::{MultiProgress, ProgressBar, ProgressStyle};

use film::{Film, Spectrum};
use program::{
    ExecutionContext, NumberInput, ProgramCompiler, ProgramFor, ProgramInput, Resources,
    VectorInput,
};
use project::{
    expressions::{Expressions, Vector},
    materials::Materials,
    meshes::Meshes,
    ProjectData,
};
use renderer::ProgressIndicator;

mod cameras;
mod film;
mod lamp;
mod light_source;
mod materials;
mod math;
mod program;
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
        let loading_started = Instant::now();

        let ProjectData {
            mut expressions,
            meshes,
            spectra,
            textures,
            materials,
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

        let parse_result = parse_project(
            project,
            programs,
            &mut expressions,
            &meshes,
            &materials,
            resources,
            &arena,
        );
        let loading_ended = Instant::now();

        match parse_result {
            Ok((image, context)) => {
                let rendering_started = Instant::now();
                render(image, context, project_path);
                let rendering_ended = Instant::now();

                println!("Done.");
                println!(
                    "Project loading: {}",
                    indicatif::FormattedDuration(loading_ended - loading_started)
                );
                println!(
                    "Rendering: {}",
                    indicatif::FormattedDuration(rendering_ended - rendering_started)
                );
                println!(
                    "Total: {}",
                    indicatif::FormattedDuration(rendering_ended - loading_started)
                );
            }
            Err(error) => eprintln!("error while parsing project: {}", error),
        }
    } else {
        eprintln!("usage: {} project_file", name);
    }
}

fn parse_project<'p>(
    project: project::Project,
    programs: ProgramCompiler<'p>,
    expressions: &mut Expressions,
    meshes: &Meshes,
    materials: &Materials,
    resources: Resources<'p>,
    arena: &'p Bump,
) -> Result<(ImageSettings<'p>, RenderContext<'p>), Box<dyn Error>> {
    let config = RenderContext {
        camera: cameras::Camera::from_project(project.camera, expressions)?,
        renderer: renderer::Renderer::from_project(project.renderer),
        world: world::World::from_project(
            project.world,
            programs,
            expressions,
            materials,
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

    let progress = MultiProgress::new();

    let progress_style =
        ProgressStyle::default_bar().template("[Thread {prefix:>3}] {bar} {percent:>3}% {msg:!}");
    let task_runner = renderer::TaskRunner {
        threads: config.renderer.threads,
        progress: ProgressIndicator {
            bars: (1..)
                .map(|id| {
                    let bar = progress.add(ProgressBar::new(0).with_style(progress_style.clone()));
                    bar.set_prefix(&id.to_string());
                    bar
                })
                .take(config.renderer.threads)
                .collect(),
        },
    };

    let global_progress = progress.add(ProgressBar::new(100)).with_style(
        ProgressStyle::default_bar()
            .template("\n{msg} {wide_bar} {percent:>3}% [{elapsed_precise}]"),
    );

    let preview_progress = progress.add(ProgressBar::new_spinner());

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
            intensity * filter_exe.run(white, &SpectrumSamplingInput { wavelength })
        }
    });

    let mut white_balance_exe = ExecutionContext::new(config.resources);
    let mut white_balance = image_settings.white.map(|white| {
        let mut wavelength = config.renderer.spectrum_span.0;
        let mut max = 0.0f32;
        let mut d65_max = 0.0f32;

        while wavelength < config.renderer.spectrum_span.1 {
            max = max.max(white_balance_exe.run(white, &SpectrumSamplingInput { wavelength }));
            d65_max = d65_max.max(light_source::D65.get(wavelength));
            wavelength += 1.0;
        }

        move |intensity: f32, wavelength: f32| {
            let white_intensity =
                white_balance_exe.run(white, &SpectrumSamplingInput { wavelength }) / max;
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

    crossbeam::thread::scope(|scope| {
        scope.spawn(|_| {
            config.renderer.render(
                &film,
                task_runner,
                |status| {
                    let time_since_print = last_print.map(|last_print| Instant::now() - last_print);

                    let should_print = time_since_print
                        .map(|time| time.as_millis() >= 500)
                        .unwrap_or(true);

                    if should_print {
                        global_progress.set_message(status.message);
                        global_progress.set_position(status.progress as u64);

                        last_print = Some(Instant::now());

                        let time_since_image = Instant::now() - last_image;
                        if time_since_image.as_secs() >= 20 {
                            let begin_iter = Instant::now();
                            preview_progress.set_message("Updating preview...");

                            for (spectrum, pixel) in
                                film.developed_pixels().zip(pixels.pixels_mut())
                            {
                                let rgb: Srgb<u8> = if let Some((red, green, blue)) = &rgb_curves {
                                    let color =
                                        spectrum_to_rgb(30.0, spectrum, &red, &green, &blue);
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

                            if let Err(e) = pixels.save(&render_path) {
                                preview_progress.finish_with_message(&format!(
                                    "Error while writing preview: {}",
                                    e
                                ));
                            } else {
                                preview_progress.finish_with_message(&format!(
                                    "Preview updated ({} seconds)",
                                    diff
                                ));
                            }
                            last_image = Instant::now();
                        }
                    }
                },
                &config.camera,
                &config.world,
                config.resources,
            );
            global_progress.finish_and_clear();
        });

        progress.join_and_clear().unwrap();
    })
    .unwrap();

    println!("Saving final result...");

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
    filter: Option<ProgramFor<'a, SpectrumSamplingInput, f32>>,
    white: Option<ProgramFor<'a, SpectrumSamplingInput, f32>>,
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
    type NumberInput = SpectrumSamplingNumberInput;
    type VectorInput = SpectrumSamplingVectorInput;

    #[inline(always)]
    fn get_number_input(&self, input: Self::NumberInput) -> f32 {
        match input {
            SpectrumSamplingNumberInput::Wavelength => self.wavelength,
        }
    }

    #[inline(always)]
    fn get_vector_input(&self, input: Self::VectorInput) -> Vector {
        match input {}
    }
}

#[derive(Clone, Copy)]
enum SpectrumSamplingNumberInput {
    Wavelength,
}

impl TryFrom<NumberInput> for SpectrumSamplingNumberInput {
    type Error = Cow<'static, str>;

    fn try_from(value: NumberInput) -> Result<Self, Self::Error> {
        match value {
            NumberInput::Wavelength => Ok(Self::Wavelength),
        }
    }
}

#[derive(Clone, Copy)]
enum SpectrumSamplingVectorInput {}

impl TryFrom<VectorInput> for SpectrumSamplingVectorInput {
    type Error = Cow<'static, str>;

    fn try_from(value: VectorInput) -> Result<Self, Self::Error> {
        match value {
            VectorInput::Normal => {
                Err("the surface normal cannot be used while sampling a constant spectrum".into())
            }
            VectorInput::Incident => {
                Err("the incident vector cannot be used while sampling a constant spectrum".into())
            }
            VectorInput::TextureCoordinates => {
                Err("texture coordinates cannot be used while sampling a constant spectrum".into())
            }
        }
    }
}
