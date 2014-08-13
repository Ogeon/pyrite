#![feature(macro_rules, struct_variant)]

extern crate cgmath;
extern crate image;

use std::sync::{TaskPool, Arc, RWLock};
use std::io::File;

use cgmath::vector::Vector2;

use image::GenericImage;

use renderer::Tile;

macro_rules! try(
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
)

mod tracer;
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
    let args = std::os::args();

    if args.len() > 1 {
        match project::from_file(Path::new(args[1].clone())) {
            project::Success(p) => render(p),
            project::IoError(e) => println!("error while reading project file: {}", e),
            project::ParseError(e) => println!("error while parsing project file: {}", e)
        }
    } else {
        println!("usage: {} project_file", args[0]);
    }
}

fn render(project: project::Project) {
    let image_size = Vector2::new(project.image.width, project.image.height);

    let tiles = project.renderer.make_tiles(&project.camera, &image_size);
    let tile_count = tiles.len();

    let config = Arc::new(RenderContext {
        camera: project.camera,
        world: project.world,
        pending: RWLock::new(tiles),
        completed: RWLock::new(Vec::new()),
        renderer: project.renderer
    });

    let mut pool = TaskPool::new(project.renderer.threads(), || {
        let config = config.clone();
        proc(id: uint) {
            (id, config)
        }
    });

    for _ in range(0, tile_count) {
        pool.execute(proc(&(task_id, ref context): &(uint, Arc<RenderContext>)) {
            let mut tile = {
                context.pending.write().pop().unwrap()
            };
            println!("Task {} got tile {}", task_id, tile.screen_area().from);

            context.renderer.render_tile(&mut tile, &context.camera, &context.world);

            context.completed.write().push(tile);
        })
    }

    let mut tile_counter = 0;

    let mut pixels = image::ImageBuf::new(image_size.x as u32, image_size.y as u32);

    let (red, green, blue) = project.image.rgb_curves;

    let red = math::Interpolated {
        points: red
    };

    let green = math::Interpolated {
        points: green
    };

    let blue = math::Interpolated {
        points: blue
    };
    
    while tile_counter < tile_count {
        std::io::timer::sleep(4000);


        loop {
            match config.completed.write().pop() {
                Some(tile) => {
                    for (spectrum, position) in tile.pixels() {
                        let r = clamp_channel(calculate_channel(&spectrum, &red));
                        let g = clamp_channel(calculate_channel(&spectrum, &green));
                        let b = clamp_channel(calculate_channel(&spectrum, &blue));
                        
                        pixels.put_pixel(position.x as u32, position.y as u32, image::Rgb(r, g, b))
                    }

                    tile_counter += 1;
                },
                None => break
            }
        }

        match File::create(&Path::new("test.png")).and_then(|f| image::ImageRgb8(pixels.clone()).save(f, image::PNG)) {
            Err(e) => println!("error while writing image: {}", e),
            _ => {}
        }
    }

    println!("Done!")
}

fn calculate_channel(spectrum: &renderer::Spectrum, response: &math::Interpolated) -> f64 {
    let mut sum = 0.0;
    let mut weight = 0.0;

    for segment in spectrum.segments() {
        let mut offset = 0.0;

        while offset < segment.width {
            let start = segment.start + offset;
            let start_resp = response.get(start);
            let end_resp = response.get(start + (segment.width - offset).min(5.0));

            let w = start_resp + end_resp;
            sum += segment.value * w;
            weight += w;

            offset += 1.0;
        }
        
    }

    sum / weight
}

struct RenderContext {
    camera: cameras::Camera,
    world: tracer::World,
    pending: RWLock<Vec<Tile>>,
    completed: RWLock<Vec<Tile>>,
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