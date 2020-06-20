# Pyrite (super pre alpha or something)

[![Build Status](https://travis-ci.org/Ogeon/pyrite.svg)](https://travis-ci.org/Ogeon/pyrite)

Pyrite is an experimental render engine, written in Rust, and only meant as a learning project. _Don't expect it to deliver the same quality as more well established renderers._

It uses various kinds of path tracing and colors based on wavelengths. The idea is to explore the possibilities this gives and to see what kinds of effects chan be achieved that way.

Some notable features:

* Spectral path tracing. Makes features like dispersion natural.
* Approximation of RGB colors, [as described by Scott Allen Burns](http://scottburns.us/fast-rgb-to-spectrum-conversion-for-reflectances/).
* Camera-to-light path tracing and bidirectional path tracing.
* Loading meshes (.obj only for now) and textures.
* 3D fractals (like quaternion Julia sets and Mandelbulbs) and other shapes, using distance estimation.
* Materials, spectra and other values can be combined as parametric values for mor customized effects.

## Getting Started

Pyrite is currently only tested on Linux, but it may work on other systems too. To download and build Pyrite using Git and Cargo:

```shell
git clone https://github.com/Ogeon/pyrite.git
cd pyrite
cargo build --release
```

To run Pyrite:

```shell
cargo run --release path/to/project.lua
```

or

```shell
target/release/pyrite path/to/project.lua
```

This will result in an image called `render.png` in `path/to/`, by default. Example projects can be found in `pyrite/test/`.

## Project Configuration

Projects are configured using Lua to get access to more flexibility and convenience than formats like JSON, YAML and TOML would provide. For example arithmetics like `spectrum(some_spectrum) * spectrum(some_other_spectrum)`, reusing values, avoiding repetition and being able to programmatically generate configuration.

The format is still in flux, so the examples in `pyrite/test/` are the best source of information (outside the renderer code) for now.

## Acknowledgements

This project uses data and a few example assets from external sources:

Data sources:

* sRGB spectra using a technique described by Scott Allen Burns: <http://scottburns.us/fast-rgb-to-spectrum-conversion-for-reflectances/>
* Data for the CIE1931 standard observer: <http://www.cvrl.org/cmfs.htm>
* Spectral data for standard illuminant D65: <https://www.rit.edu/cos/colorscience/rc_useful_data.php>

Example assets:

* Cornell Box: <http://www.graphics.cornell.edu/online/box/data.html>
* Stanford Dragon: <http://graphics.stanford.edu/data/3Dscanrep/>
* ColorChecker image: <https://commons.wikimedia.org/wiki/File:ColorChecker100423.jpg>
* Tile floor textures: <https://cc0textures.com/view?id=Tiles012>

## Contributing

Pyrite exists because it's fun to write and experiment with and you are most welcome to help if you feel like it would be a fun thing to do. Optimizations, beautiful materials, response curves, cool shapes or other improvements are all welcome. Suggestions and interesting reading material is also welcome!

## License

Licensed under either of

* Apache License, Version 2.0, ([LICENSE-APACHE](LICENSE-APACHE) or <http://www.apache.org/licenses/LICENSE-2.0>)
* MIT license ([LICENSE-MIT](LICENSE-MIT) or <http://opensource.org/licenses/MIT>)

at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in the work by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.
