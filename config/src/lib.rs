mod parser;
mod lexer;

use std::path::Path;
use std::fs::File;
use std::io::Read;

pub enum Error {
    Parse(parser::Error),
    Io(std::io::Error)
}

impl From<parser::Error> for Error {
    fn from(e: parser::Error) -> Error {
        Error::Parse(e)
    }
}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Error {
        Error::Io(e)
    }
}

pub fn parse_file<P: AsRef<Path>>(path: P) -> Result<(), Error> {
    structure(path)
}

fn structure<P: AsRef<Path>>(path: P) -> Result<(), Error> {
    let mut source = String::new();
    let mut file = try!(File::open(&path));
    try!(file.read_to_string(&mut source));
    let statements = try!(parser::parse(source.chars()));
    Ok(())
}



