use std::{
    fs::File,
    io::BufReader,
    ops::{Add, Mul, Sub},
    path::Path,
};

use cgmath::Point2;
use palette::{
    white_point::D65, Alpha, Component, FromColor, IntoComponent, LinLuma, LinLumaa, LinSrgb,
    LinSrgba, Pixel, Srgb, SrgbLuma, SrgbLumaa, Srgba,
};

/// Linearized image data.
pub struct Texture<T> {
    width: usize,
    height: usize,
    data: Vec<T>,
}

impl<T> Texture<T> {
    pub fn from_path<P: AsRef<Path>>(path: P, linear: bool) -> image::ImageResult<Texture<T>>
    where
        T: FromColor<LinLuma> + FromColor<LinLumaa> + FromColor<LinSrgb> + FromColor<LinSrgba>,
    {
        use image::GenericImageView;

        let path = path.as_ref();
        let image = image::load(
            BufReader::new(File::open(path)?),
            image::ImageFormat::from_path(path)?,
        )?;

        let (width, height) = image.dimensions();
        let data = match image {
            image::DynamicImage::ImageLuma8(image) => {
                convert_pixels::<SrgbLuma<u8>, _, _>(&image.into_raw(), linear)
            }
            image::DynamicImage::ImageLumaA8(image) => {
                convert_pixels::<SrgbLumaa<u8>, _, _>(&image.into_raw(), linear)
            }
            image::DynamicImage::ImageRgb8(image) => {
                convert_pixels::<Srgb<u8>, _, _>(&image.into_raw(), linear)
            }
            image::DynamicImage::ImageRgba8(image) => {
                convert_pixels::<Srgba<u8>, _, _>(&image.into_raw(), linear)
            }
            image::DynamicImage::ImageBgr8(image) => {
                convert_pixels::<Bgr, _, _>(&image.into_raw(), linear)
            }
            image::DynamicImage::ImageBgra8(image) => {
                convert_pixels::<Alpha<Bgr, _>, _, _>(&image.into_raw(), linear)
            }
            image::DynamicImage::ImageLuma16(image) => {
                convert_pixels::<SrgbLuma<u16>, _, _>(&image.into_raw(), linear)
            }
            image::DynamicImage::ImageLumaA16(image) => {
                convert_pixels::<SrgbLumaa<u16>, _, _>(&image.into_raw(), linear)
            }
            image::DynamicImage::ImageRgb16(image) => {
                convert_pixels::<Srgb<u16>, _, _>(&image.into_raw(), linear)
            }
            image::DynamicImage::ImageRgba16(image) => {
                convert_pixels::<Srgba<u16>, _, _>(&image.into_raw(), linear)
            }
        };

        Ok(Texture {
            width: width as usize,
            height: height as usize,
            data,
        })
    }

    pub fn get_color(&self, position: Point2<f32>) -> T
    where
        T: Copy + Add<Output = T> + Sub<Output = T> + Mul<Output = T> + Mul<f32, Output = T>,
    {
        let width_f = self.width as f32;
        let height_f = self.height as f32;

        let x = position.x * width_f - 0.5;
        let x2 = x.floor() as i64;
        let x1 = x2 - 1;
        let x3 = x2 + 1;
        let x4 = x2 + 2;

        let y = 1.0 - (position.y * height_f - 0.5);
        let y2 = y.floor() as i64;
        let y1 = y2 - 1;
        let y3 = y2 + 1;
        let y4 = y2 + 2;

        let x = x.rem_euclid(1.0);
        let x1 = x1.rem_euclid(self.width as i64) as usize;
        let x2 = x2.rem_euclid(self.width as i64) as usize;
        let x3 = x3.rem_euclid(self.width as i64) as usize;
        let x4 = x4.rem_euclid(self.width as i64) as usize;

        let y = y.rem_euclid(1.0);
        let y1 = y1.rem_euclid(self.height as i64) as usize;
        let y2 = y2.rem_euclid(self.height as i64) as usize;
        let y3 = y3.rem_euclid(self.height as i64) as usize;
        let y4 = y4.rem_euclid(self.height as i64) as usize;

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

    #[inline(always)]
    fn color_at(&self, x: usize, y: usize) -> T
    where
        T: Copy,
    {
        self.data[x + y * self.width]
    }
}

fn convert_pixels<A, B, T>(pixels: &[T], linear: bool) -> Vec<B>
where
    A: SourceColor + Pixel<T> + Copy,
    A::LinearSourceColor: Pixel<T> + Copy,
    T: Component,
    B: FromColor<A::LinearFloats>,
{
    if linear {
        let pixels = A::LinearSourceColor::from_raw_slice(pixels);
        pixels
            .into_iter()
            .map(|&pixel| pixel.into_linear_floats())
            .map(B::from_color)
            .collect::<Vec<_>>()
    } else {
        let pixels = A::from_raw_slice(pixels);
        pixels
            .into_iter()
            .map(|&pixel| pixel.into_linear_floats())
            .map(B::from_color)
            .collect::<Vec<_>>()
    }
}

trait SourceColor: IntoLinearFloats {
    type LinearSourceColor: IntoLinearFloats<LinearFloats = Self::LinearFloats>;
}

trait IntoLinearFloats {
    type LinearFloats: Pixel<f32>;

    fn into_linear_floats(self) -> Self::LinearFloats;
}

impl<T: Component + IntoComponent<f32>> SourceColor for Srgb<T> {
    type LinearSourceColor = LinSrgb<T>;
}

impl<T: Component + IntoComponent<f32>> IntoLinearFloats for Srgb<T> {
    type LinearFloats = LinSrgb;

    fn into_linear_floats(self) -> Self::LinearFloats {
        self.into_format().into_linear()
    }
}

impl<T: Component + IntoComponent<f32>> IntoLinearFloats for LinSrgb<T> {
    type LinearFloats = LinSrgb;

    fn into_linear_floats(self) -> Self::LinearFloats {
        self.into_format()
    }
}

impl<T: Component + IntoComponent<f32>> SourceColor for SrgbLuma<T> {
    type LinearSourceColor = LinLuma<D65, T>;
}

impl<T: Component + IntoComponent<f32>> IntoLinearFloats for SrgbLuma<T> {
    type LinearFloats = LinLuma;

    fn into_linear_floats(self) -> Self::LinearFloats {
        self.into_format().into_linear()
    }
}

impl<T: Component + IntoComponent<f32>> IntoLinearFloats for LinLuma<D65, T> {
    type LinearFloats = LinLuma;

    fn into_linear_floats(self) -> Self::LinearFloats {
        self.into_format()
    }
}

impl<C: SourceColor, T: Component + IntoComponent<f32>> SourceColor for Alpha<C, T> {
    type LinearSourceColor = Alpha<C::LinearSourceColor, T>;
}

impl<C: IntoLinearFloats, T: Component + IntoComponent<f32>> IntoLinearFloats for Alpha<C, T> {
    type LinearFloats = Alpha<C::LinearFloats, f32>;

    fn into_linear_floats(self) -> Self::LinearFloats {
        Alpha {
            color: self.color.into_linear_floats(),
            alpha: self.alpha.into_component(),
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
    type LinearSourceColor = LinBgr;
}

impl IntoLinearFloats for Bgr {
    type LinearFloats = LinSrgb;

    fn into_linear_floats(self) -> Self::LinearFloats {
        Srgb::new(self.red, self.green, self.blue).into_linear_floats()
    }
}

#[derive(Pixel, Clone, Copy)]
#[repr(C)]
struct LinBgr {
    blue: u8,
    green: u8,
    red: u8,
}

impl IntoLinearFloats for LinBgr {
    type LinearFloats = LinSrgb;

    fn into_linear_floats(self) -> Self::LinearFloats {
        LinSrgb::new(self.red, self.green, self.blue).into_linear_floats()
    }
}

fn bicubic_interpolate<T>(points: [[T; 4]; 4], pos_x: f32, pos_y: f32) -> T
where
    T: Copy + Add<Output = T> + Sub<Output = T> + Mul<Output = T> + Mul<f32, Output = T>,
{
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

fn cubic_interpolate<T>(v1: T, v2: T, v3: T, v4: T, pos: f32) -> T
where
    T: Copy + Add<Output = T> + Sub<Output = T> + Mul<Output = T> + Mul<f32, Output = T>,
{
    let a = (v4 - v3) - (v1 - v2);
    let b = (v1 - v2) - a;
    let c = v3 - v1;
    let d = v2;

    d + (c + (b + a * pos) * pos) * pos
}
