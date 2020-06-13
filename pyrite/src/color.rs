use palette::{LinSrgb, Srgb};

use pyrite_config::{entry::Entry, Prelude, Value};

use crate::{
    math::{utils::Interpolated, Math, RenderMath},
    texture::Texture,
    tracer::{ParametricValue, RenderContext},
};
use std::path::Path;

pub enum Color {
    Spectrum(Interpolated),
    Rgb(LinSrgb),
    Constant(f32),
    Texture(Texture),
}

impl ParametricValue<RenderContext, f32> for Color {
    fn get(&self, context: &RenderContext) -> f32 {
        match self {
            Color::Spectrum(interpolated) => interpolated.get(context.wavelength),
            &Color::Rgb(LinSrgb {
                red, green, blue, ..
            }) => {
                let wavelength = context.wavelength;

                let red_response = red * crate::rgb::response::RED.get(wavelength);
                let green_response = green * crate::rgb::response::GREEN.get(wavelength);
                let blue_response = blue * crate::rgb::response::BLUE.get(wavelength);

                red_response + green_response + blue_response
            }
            Color::Constant(constant) => *constant,
            Color::Texture(texture) => {
                let position = context.texture;
                let LinSrgb {
                    red, green, blue, ..
                } = texture.get_color(position).color;
                let wavelength = context.wavelength;

                let red_response = red * crate::rgb::response::RED.get(wavelength);
                let green_response = green * crate::rgb::response::GREEN.get(wavelength);
                let blue_response = blue * crate::rgb::response::BLUE.get(wavelength);

                red_response + green_response + blue_response
            }
        }
    }
}

impl From<f32> for Color {
    fn from(constant: f32) -> Self {
        Color::Constant(constant)
    }
}

pub fn register_types(context: &mut Prelude) {
    let mut object = context.object("Color".into());

    {
        let mut object = object.object("Spectrum".into());
        object.add_decoder(decode_spectrum);
        object.arguments(vec!["points".into()]);
    }

    {
        let mut object = object.object("Rgb".into());
        object.add_decoder(decode_rgb);
        object.arguments(vec!["red".into(), "green".into(), "blue".into()]);
    }

    {
        let mut object = object.object("Texture".into());
        object.add_decoder(decode_texture);
        object.arguments(vec!["file_path".into()]);
    }
}

pub fn decode_color(_path: &'_ Path, entry: Entry<'_>) -> Result<RenderMath<Color>, String> {
    if let Some(&Value::Number(num)) = entry.as_value() {
        Ok(Math::Value(Color::Constant(num.as_float())))
    } else {
        entry.dynamic_decode()
    }
}

fn decode_spectrum(_path: &'_ Path, entry: Entry<'_>) -> Result<RenderMath<Color>, String> {
    let fields = entry.as_object().ok_or("not an object")?;

    let points = match fields.get("points") {
        Some(v) => try_for!(v.decode(), "points"),
        None => return Err("missing field 'points'".into()),
    };

    Ok(Math::Value(Color::Spectrum(Interpolated { points })))
}

fn decode_rgb(_path: &'_ Path, entry: Entry<'_>) -> Result<RenderMath<Color>, String> {
    let fields = entry.as_object().ok_or("not an object")?;

    let red = match fields.get("red") {
        Some(v) => try_for!(v.decode(), "red"),
        None => return Err("missing field 'red'".into()),
    };

    let green = match fields.get("green") {
        Some(v) => try_for!(v.decode(), "green"),
        None => return Err("missing field 'green'".into()),
    };

    let blue = match fields.get("blue") {
        Some(v) => try_for!(v.decode(), "blue"),
        None => return Err("missing field 'blue'".into()),
    };

    Ok(Math::Value(Color::Rgb(
        Srgb::new(red, green, blue).into_linear(),
    )))
}

fn decode_texture(path: &'_ Path, entry: Entry<'_>) -> Result<RenderMath<Color>, String> {
    let fields = entry.as_object().ok_or("not an object")?;

    let file_path: String = match fields.get("file_path") {
        Some(v) => try_for!(v.decode(), "file_path"),
        None => return Err("missing field 'file_path'".into()),
    };

    let texture = Texture::from_path(path.join(file_path)).map_err(|error| error.to_string())?;

    Ok(Math::Value(Color::Texture(texture)))
}
