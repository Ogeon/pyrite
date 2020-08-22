use std::{error::Error, fs, path::Path};

use serde::Deserialize;

use quote::quote;

fn main() -> Result<(), Box<dyn Error>> {
    let out_dir = std::env::var_os("OUT_DIR").unwrap();

    read_rgb_response(&Path::new(&out_dir))?;
    read_xyz_response(&Path::new(&out_dir))?;
    read_light_sources(&Path::new(&out_dir))?;

    println!("cargo:rerun-if-changed=build.rs");
    Ok(())
}

fn read_rgb_response(out_dir: &Path) -> Result<(), Box<dyn Error>> {
    let mut rgb_response = vec![];
    let mut r_response = vec![];
    let mut g_response = vec![];
    let mut b_response = vec![];

    // Uses data from http://scottburns.us/fast-rgb-to-spectrum-conversion-for-reflectances/
    println!("cargo:rerun-if-changed=data/srgb_cie1931.csv");
    let mut reader = csv::Reader::from_path("data/srgb_cie1931.csv")?;
    for record_result in reader.deserialize() {
        let RgbResponse { r, g, b } = record_result?;
        rgb_response.push(
            quote! { LinSrgb { red: #r, green: #g, blue: #b, standard: std::marker::PhantomData } },
        );
        r_response.push(quote!(#r));
        g_response.push(quote!(#g));
        b_response.push(quote!(#b));
    }

    let min_wavelength = 360.0;
    let max_wavelength = min_wavelength + r_response.len() as f32;

    fs::write(
        out_dir.join("rgb_response.rs"),
        quote! {
            pub mod response {
                use std::borrow::Cow;
                use palette::LinSrgb;
                use crate::project::spectra::Spectrum;

                pub const RGB: Spectrum<LinSrgb> = Spectrum::Array{
                    min: #min_wavelength,
                    max: #max_wavelength,
                    points: Cow::Borrowed(&[#(#rgb_response),*])
                };
            }
        }
        .to_string(),
    )?;

    Ok(())
}

#[derive(Debug, Deserialize)]
struct RgbResponse {
    r: f32,
    g: f32,
    b: f32,
}

fn read_xyz_response(out_dir: &Path) -> Result<(), Box<dyn Error>> {
    let mut x_response = vec![];
    let mut y_response = vec![];
    let mut z_response = vec![];

    let mut min_wavelength = std::f32::INFINITY;
    let mut max_wavelength = 0.0f32;

    println!("cargo:rerun-if-changed=data/ciexyz65_1.csv");
    for record_result in csv::Reader::from_path("data/ciexyz65_1.csv")?.deserialize() {
        let XyzResponse {
            wavelength,
            x,
            y,
            z,
        } = record_result?;

        min_wavelength = min_wavelength.min(wavelength);
        max_wavelength = max_wavelength.max(wavelength);

        x_response.push(quote!(#x));
        y_response.push(quote!(#y));
        z_response.push(quote!(#z));
    }

    fs::write(
        out_dir.join("xyz_response.rs"),
        quote! {
            pub mod response {
                use std::borrow::Cow;
                use crate::project::spectra::Spectrum;

                pub const X: Spectrum<f32> = Spectrum::Array {
                    min: #min_wavelength,
                    max: #max_wavelength,
                    points: Cow::Borrowed(&[#(#x_response),*])
                };
                pub const Y: Spectrum<f32> = Spectrum::Array {
                    min: #min_wavelength,
                    max: #max_wavelength,
                    points: Cow::Borrowed(&[#(#y_response),*])
                };
                pub const Z: Spectrum<f32> = Spectrum::Array {
                    min: #min_wavelength,
                    max: #max_wavelength,
                    points: Cow::Borrowed(&[#(#z_response),*])
                };
            }
        }
        .to_string(),
    )?;

    Ok(())
}

#[derive(Debug, Deserialize)]
struct XyzResponse {
    wavelength: f32,
    x: f32,
    y: f32,
    z: f32,
}

fn read_light_sources(out_dir: &Path) -> Result<(), Box<dyn Error>> {
    let mut d65_spectrum = vec![];
    let mut a_spectrum = vec![];

    let mut min_wavelength = std::f32::INFINITY;
    let mut max_wavelength = 0.0f32;

    println!("cargo:rerun-if-changed=data/d65.csv");
    for record_result in csv::Reader::from_path("data/d65.csv")?.deserialize() {
        let LightIntensity {
            wavelength,
            intensity,
        } = record_result?;

        min_wavelength = min_wavelength.min(wavelength);
        max_wavelength = max_wavelength.max(wavelength);

        d65_spectrum.push(quote!(#intensity));
    }

    println!("cargo:rerun-if-changed=data/a.csv");
    for record_result in csv::Reader::from_path("data/a.csv")?.deserialize() {
        let LightIntensity {
            wavelength,
            intensity,
        } = record_result?;
        let intensity = intensity / 100.0;

        min_wavelength = min_wavelength.min(wavelength);
        max_wavelength = max_wavelength.max(wavelength);

        a_spectrum.push(quote!(#intensity));
    }

    fs::write(
        out_dir.join("light_source.rs"),
        quote! {
                use std::borrow::Cow;
                use crate::project::spectra::Spectrum;

                pub const D65: Spectrum<f32> = Spectrum::Array {
                    min: #min_wavelength,
                    max: #max_wavelength,
                    points: Cow::Borrowed(&[#(#d65_spectrum),*])
                };

                pub const A: Spectrum<f32> = Spectrum::Array {
                    min: #min_wavelength,
                    max: #max_wavelength,
                    points: Cow::Borrowed(&[#(#a_spectrum),*])
                };
        }
        .to_string(),
    )?;

    Ok(())
}

#[derive(Debug, Deserialize)]
struct LightIntensity {
    wavelength: f32,
    intensity: f32,
}
