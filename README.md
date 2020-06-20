# Pyrite (super pre alpha or something)

[![Build Status](https://travis-ci.org/Ogeon/pyrite.svg)](https://travis-ci.org/Ogeon/pyrite)

Pyrite is an experimental render engine, written in Rust, and only meant as a learning project. _Don't expect it to deliver the same quality as more well established renderers._

It uses various kinds of path tracing and colors based on wavelengths. The idea is to explore the possibilities this gives and to see what kinds of effects chan be achieved that way.

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

## Contributing

Pyrite exists because it's fun to write and experiment with and you are most welcome to help if you feel
like it would be a fun thing to do. Optimizations, beautiful materials, response curves, cool shapes or
other improvements are all welcome. Just make sure things works by running the tests and maybe add a new test
for your feature.
