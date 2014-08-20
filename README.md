# Pyrite (super pre alpha or something)
Pyrite is an experimental render engine, written in Rust. It uses path
tracing and colors based on wavelengths.

## Getting Started
Pyrite is currently only tested on Linux, but it may work on other systems too.
To download and build Pyrite using Git and Cargo:

    git clone https://github.com/Ogeon/pyrite.git
    cd pyrite
    cargo build --release

To run Pyrite:

    target/release/pyrite path/to/project.pyr

This will result in an image called `render.png` in `path/to/`. Example
projects can be found in `test/`.

## Dependencies
Pyrite uses the following libraries:

* [cgmath-rs](https://github.com/bjz/cgmath-rs) for linear algebra.
* [rust-image](https://github.com/PistonDevelopers/rust-image) for saving and loading image images.
* [wavefront-obj](https://github.com/PistonDevelopers/wavefront-obj) for loading `.obj` files.

They are automatically downloaded and built by Cargo, so don't worry.

## Project Configuration
The configuration language for the project files was created especially for the needs of Pyrite.
The reason why it's used instead of an other language, like JSON, YAML or TOML, is both that this kind of
application has some special needs which may be hard for some languages to fulfill and that the more
powerful languages can be a bit too big or just not human friendly enough.

This language has an optional type system which allows some parts of the project files to be strictly defined,
while other parts can be a bit more dynamic. It allows structures to be built in several steps and one structure
can use an other one as a template to minimize repetitions.

Study the demo files in `test/` to see it in action.

## Contributing
Pyrite exists because it's fun to write and experiment with and you are most welcome to help if you feel
like it would be a fun thing to do. Optimizations, beautiful materials, response curves, cool shapes or
other improvements are all welcome. Just make sure things works by running the tests and maybe add a new test
for your feature.
