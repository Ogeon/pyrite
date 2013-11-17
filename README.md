# Pyrite (super pre alpha)
Pyrite is an experimental render engine, written in Rust. It will use path
tracing and colors based on wavelengths.

## Getting started
Pyrite is currently only tested on Linux, but it may work on other systems too.

To download and build Pyrite to the `bin/` folder:


    git clone https://github.com/Ogeon/pyrite.git
    cd pyrite
    make

To run Pyrite:


    cd bin/
    ./pyrite path/to/project.json

This will result in an image called `render.png` in `path/to/`. Example
projects can be found in `test/`.

## Dependencies
Pyrite requires the following libraries:

* [nalgebra](https://github.com/sebcrozet/nalgebra) for linear algebra.
* [rust-png](https://github.com/mozilla-servo/rust-png) for saving and loading PNG images.