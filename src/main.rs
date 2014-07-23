extern crate cgmath;
extern crate image;

use std::cmp::min;
use std::sync::{TaskPool, Arc, RWLock};
use std::io::File;

use cgmath::vector::{Vector2, Vector3};
use cgmath::rotation::Rotation;
use cgmath::transform::{Transform, Decomposed};
use cgmath::angle::deg;
use cgmath::ray::Ray3;

use tracer::{Camera, Area, Tile};

mod tracer;
mod cameras;
mod worlds;
mod shapes;

fn main() {
    let tile_size = 64;
    let image_size = Vector2::new(640, 480);
    let samples = 10;

    let camera = cameras::Perspective::new(Transform::identity(), image_size.clone(), deg(45.0f64));

    let tiles_x = (image_size.x as f32 / tile_size as f32).ceil() as uint;
    let tiles_y = (image_size.y as f32 / tile_size as f32).ceil() as uint;
    let tile_count = tiles_x * tiles_y;

    let mut tiles = Vec::new();

    for y in range(0, tiles_y) {
        for x in range(0, tiles_x) {
            let from = Vector2::new(x * tile_size, y * tile_size);
            let size = Vector2::new(min(image_size.x - from.x, tile_size), min(image_size.y - from.y, tile_size));

            let image_area = Area::new(from, size);
            let camera_area = camera.to_view_area(&image_area);

            tiles.push(Tile::new(image_area, camera_area, 0.0, 1.0, 1));
        }
    }

    let sphere = shapes::Sphere(
        Decomposed {
            scale: 1.0,
            rot: Rotation::identity(),
            disp: Vector3::new(0.0, 0.0, 2.0)
        }
    );

    let config = Arc::new(RenderContext {
        camera: camera,
        world: worlds::SimpleWorld::new(vec!(Geometric(sphere))),
        pending: RWLock::new(tiles),
        completed: RWLock::new(Vec::new())
    });

    let mut pool = TaskPool::new(std::rt::default_sched_threads(), || {
        let config = config.clone();
        proc(id: uint) {
            (id, config)
        }
    });

    for _ in range(0, tile_count) {
        pool.execute(proc(&(task_id, ref context): &(uint, Arc<RenderContext<cameras::Perspective, worlds::SimpleWorld<Vec<Object>>>>)) {
            let mut tile = {
                context.pending.write().pop().unwrap()
            };
            println!("Task {} got tile {}", task_id, tile.screen_area().from);

            tracer::render(&mut tile, samples, &context.camera, &context.world);

            context.completed.write().push(tile);
        })
    }

    let mut tile_counter = 0;

    let mut pixels = Vec::from_elem(image_size.x * image_size.y * 3, 0);
    
    while tile_counter < tile_count {
        std::io::timer::sleep(4000);


        loop {
            match config.completed.write().pop() {
                Some(tile) => {
                    for (spectrum, position) in tile.pixels() {
                        let value = clamp_channel(spectrum.value_at(0.0));
                        *pixels.get_mut(position.x * 3 + position.y * image_size.x * 3)     = value;
                        *pixels.get_mut(position.x * 3 + position.y * image_size.x * 3 + 1) = value;
                        *pixels.get_mut(position.x * 3 + position.y * image_size.x * 3 + 2) = value;
                    }

                    tile_counter += 1;
                },
                None => break
            }
        }

        let mut encoder = image::PNGEncoder::new(File::create(&Path::new("test.png")));
        match encoder.encode(pixels.as_slice(), image_size.x as u32, image_size.y as u32, image::RGB(8)) {
            Err(e) => println!("error while writing image: {}", e),
            _ => {}
        }
    }

    println!("Done!")
}

struct RenderContext<C, W> {
    camera: C,
    world: W,
    pending: RWLock<Vec<Tile>>,
    completed: RWLock<Vec<Tile>>
}

enum Object {
    Geometric(shapes::Shape)
}

impl worlds::WorldObject for Object {
    fn intersect(&self, ray: &Ray3<f64>) -> Option<Ray3<f64>> {
        match *self {
            Geometric(shape) => shape.intersect(ray)
        }
    }
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