use std::{error::Error, fs, path::Path};

use serde::Deserialize;

use quote::quote;

fn main() -> Result<(), Box<dyn Error>> {
    let out_dir = std::env::var_os("OUT_DIR").unwrap();

    read_rgb_response(&Path::new(&out_dir))?;
    read_xyz_response(&Path::new(&out_dir))?;

    println!("cargo:rerun-if-changed=build.rs");
    Ok(())
}

fn read_rgb_response(out_dir: &Path) -> Result<(), Box<dyn Error>> {
    let mut r_response = vec![];
    let mut g_response = vec![];
    let mut b_response = vec![];

    // Uses data from http://scottburns.us/fast-rgb-to-spectrum-conversion-for-reflectances/
    println!("cargo:rerun-if-changed=data/srgb_cie1931.csv");
    let mut reader = csv::Reader::from_path("data/srgb_cie1931.csv")?;
    for (offset, record_result) in reader.deserialize().enumerate() {
        let wavelength = (360 + offset) as f32;
        let RgbResponse { r, g, b } = record_result?;
        r_response.push(quote!((#wavelength, #r)));
        g_response.push(quote!((#wavelength, #g)));
        b_response.push(quote!((#wavelength, #b)));
    }

    fs::write(
        out_dir.join("rgb_response.rs"),
        quote! {
            pub mod response {
                use crate::math::utils::Interpolated;

                pub const RED: Interpolated<&[(f32, f32)]> = Interpolated{ points: &[#(#r_response),*] };
                pub const GREEN: Interpolated<&[(f32, f32)]> = Interpolated{ points: &[#(#g_response),*] };
                pub const BLUE: Interpolated<&[(f32, f32)]> = Interpolated{ points: &[#(#b_response),*] };
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

    println!("cargo:rerun-if-changed=data/ciexyz64_1.csv");
    for record_result in csv::Reader::from_path("data/ciexyz64_1.csv")?.deserialize() {
        let XyzResponse {
            wavelength,
            x,
            y,
            z,
        } = record_result?;
        x_response.push(quote!((#wavelength, #x)));
        y_response.push(quote!((#wavelength, #y)));
        z_response.push(quote!((#wavelength, #z)));
    }

    fs::write(
        out_dir.join("xyz_response.rs"),
        quote! {
            pub mod response {
                pub const X: &[(f32, f32)] = &[#(#x_response),*];
                pub const Y: &[(f32, f32)] = &[#(#y_response),*];
                pub const Z: &[(f32, f32)] = &[#(#z_response),*];
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
