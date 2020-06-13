use std::{fs::File, io::BufReader, path::Path};

use cgmath::Point2;
use palette::{
    Alpha, Component, LinLuma, LinLumaa, LinSrgb, LinSrgba, Pixel, Srgb, SrgbLuma, SrgbLumaa, Srgba,
};

/// Linearized image data.
pub struct Texture {
    format: TextureFormat,
    width: usize,
    height: usize,
    data: Vec<f32>,
}

impl Texture {
    pub fn from_path<P: AsRef<Path>>(path: P) -> image::ImageResult<Texture> {
        use image::GenericImageView;

        let path = path.as_ref();
        let image = image::load(
            BufReader::new(File::open(path)?),
            image::ImageFormat::from_path(path)?,
        )?;

        let (width, height) = image.dimensions();
        let (format, data) = match image {
            image::DynamicImage::ImageLuma8(image) => (
                TextureFormat::Mono,
                convert_pixels::<SrgbLuma<u8>, _>(&image.into_raw()),
            ),
            image::DynamicImage::ImageLumaA8(image) => (
                TextureFormat::MonoAlpha,
                convert_pixels::<SrgbLumaa<u8>, _>(&image.into_raw()),
            ),
            image::DynamicImage::ImageRgb8(image) => (
                TextureFormat::Rgb,
                convert_pixels::<Srgb<u8>, _>(&image.into_raw()),
            ),
            image::DynamicImage::ImageRgba8(image) => (
                TextureFormat::RgbAlpha,
                convert_pixels::<Srgba<u8>, _>(&image.into_raw()),
            ),
            image::DynamicImage::ImageBgr8(image) => (
                TextureFormat::Rgb,
                convert_pixels::<Bgr, _>(&image.into_raw()),
            ),
            image::DynamicImage::ImageBgra8(image) => (
                TextureFormat::RgbAlpha,
                convert_pixels::<Alpha<Bgr, _>, _>(&image.into_raw()),
            ),
            image::DynamicImage::ImageLuma16(image) => (
                TextureFormat::Mono,
                convert_pixels::<SrgbLuma<u16>, _>(&image.into_raw()),
            ),
            image::DynamicImage::ImageLumaA16(image) => (
                TextureFormat::MonoAlpha,
                convert_pixels::<SrgbLumaa<u16>, _>(&image.into_raw()),
            ),
            image::DynamicImage::ImageRgb16(image) => (
                TextureFormat::Rgb,
                convert_pixels::<Srgb<u16>, _>(&image.into_raw()),
            ),
            image::DynamicImage::ImageRgba16(image) => (
                TextureFormat::RgbAlpha,
                convert_pixels::<Srgba<u16>, _>(&image.into_raw()),
            ),
        };

        Ok(Texture {
            format,
            width: width as usize,
            height: height as usize,
            data,
        })
    }

    pub fn get_color(&self, position: Point2<f32>) -> LinSrgba {
        let width_f = self.width as f32;
        let height_f = self.height as f32;

        let x = position.x * width_f - 0.5;
        let x2 = x.floor();
        let x1 = x2 - 1.0;
        let x3 = x2 + 1.0;
        let x4 = x2 + 2.0;

        let y = 1.0 - (position.y * height_f - 0.5);
        let y2 = y.floor();
        let y1 = y2 - 1.0;
        let y3 = y2 + 1.0;
        let y4 = y2 + 2.0;

        let x = x.rem_euclid(1.0);
        let x1 = (x1.rem_euclid(width_f) as usize).min(self.width - 1);
        let x2 = (x2.rem_euclid(width_f) as usize).min(self.width - 1);
        let x3 = (x3.rem_euclid(width_f) as usize).min(self.width - 1);
        let x4 = (x4.rem_euclid(width_f) as usize).min(self.width - 1);

        let y = y.rem_euclid(1.0);
        let y1 = (y1.rem_euclid(height_f) as usize).min(self.height - 1);
        let y2 = (y2.rem_euclid(height_f) as usize).min(self.height - 1);
        let y3 = (y3.rem_euclid(height_f) as usize).min(self.height - 1);
        let y4 = (y4.rem_euclid(height_f) as usize).min(self.height - 1);

        let points = [
            [
                self.color_at(x1, y1),
                self.color_at(x2, y1),
                self.color_at(x3, y1),
                self.color_at(x4, y1),
            ],
            [
                self.color_at(x1, y2),
                self.color_at(x2, y2),
                self.color_at(x3, y2),
                self.color_at(x4, y2),
            ],
            [
                self.color_at(x1, y3),
                self.color_at(x2, y3),
                self.color_at(x3, y3),
                self.color_at(x4, y3),
            ],
            [
                self.color_at(x1, y4),
                self.color_at(x2, y4),
                self.color_at(x3, y4),
                self.color_at(x4, y4),
            ],
        ];

        bicubic_interpolate(points, x, y)
    }

    fn color_at(&self, x: usize, y: usize) -> LinSrgba {
        let index = x + y * self.width;

        match self.format {
            TextureFormat::Mono => LinLuma::from_raw_slice(&self.data)[index].into(),
            TextureFormat::MonoAlpha => LinLumaa::from_raw_slice(&self.data)[index].into(),
            TextureFormat::Rgb => LinSrgb::from_raw_slice(&self.data)[index].into(),
            TextureFormat::RgbAlpha => LinSrgba::from_raw_slice(&self.data)[index],
        }
    }
}

fn convert_pixels<C, T>(pixels: &[T]) -> Vec<f32>
where
    C: SourceColor + Pixel<T> + Copy,
    T: Component,
{
    let pixels = C::from_raw_slice(pixels);
    let linear_pixels = pixels
        .into_iter()
        .map(|&pixel| pixel.into_linear_floats())
        .collect::<Vec<_>>();

    Pixel::into_raw_slice(&linear_pixels).into()
}

trait SourceColor {
    type LinearFloats: Pixel<f32>;

    fn into_linear_floats(self) -> Self::LinearFloats;
}

impl<T: Component> SourceColor for Srgb<T> {
    type LinearFloats = LinSrgb;

    fn into_linear_floats(self) -> Self::LinearFloats {
        self.into_format().into_linear()
    }
}

impl<T: Component> SourceColor for SrgbLuma<T> {
    type LinearFloats = LinLuma;

    fn into_linear_floats(self) -> Self::LinearFloats {
        self.into_format().into_linear()
    }
}

impl<C: SourceColor, T: Component> SourceColor for Alpha<C, T> {
    type LinearFloats = Alpha<C::LinearFloats, f32>;

    fn into_linear_floats(self) -> Self::LinearFloats {
        Alpha {
            color: self.color.into_linear_floats(),
            alpha: self.alpha.convert(),
        }
    }
}

#[derive(Pixel, Clone, Copy)]
#[repr(C)]
struct Bgr {
    blue: u8,
    green: u8,
    red: u8,
}

impl SourceColor for Bgr {
    type LinearFloats = LinSrgb;

    fn into_linear_floats(self) -> Self::LinearFloats {
        Srgb::new(self.red, self.green, self.blue).into_linear_floats()
    }
}

enum TextureFormat {
    Mono,
    MonoAlpha,
    Rgb,
    RgbAlpha,
}

fn bicubic_interpolate(points: [[LinSrgba; 4]; 4], pos_x: f32, pos_y: f32) -> LinSrgba {
    let [row1, row2, row3, row4] = points;

    let [v1, v2, v3, v4] = row1;
    let x1 = cubic_interpolate(v1, v2, v3, v4, pos_x);

    let [v1, v2, v3, v4] = row2;
    let x2 = cubic_interpolate(v1, v2, v3, v4, pos_x);

    let [v1, v2, v3, v4] = row3;
    let x3 = cubic_interpolate(v1, v2, v3, v4, pos_x);

    let [v1, v2, v3, v4] = row4;
    let x4 = cubic_interpolate(v1, v2, v3, v4, pos_x);

    cubic_interpolate(x1, x2, x3, x4, pos_y)
}

fn cubic_interpolate(v1: LinSrgba, v2: LinSrgba, v3: LinSrgba, v4: LinSrgba, pos: f32) -> LinSrgba {
    let a = (v4 - v3) - (v1 - v2);
    let b = (v1 - v2) - a;
    let c = v3 - v1;
    let d = v2;

    d + (c + (b + a * pos) * pos) * pos
}
