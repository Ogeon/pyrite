use std::{error::Error, fs, path::Path};

use serde::Deserialize;

use quote::quote;

fn main() -> Result<(), Box<dyn Error>> {
    let out_dir = std::env::var_os("OUT_DIR").unwrap();

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
        &Path::new(&out_dir).join("xyz_response.rs"),
        quote! {
            pub mod response {
                pub const X: &[(f32, f32)] = &[#(#x_response),*];
                pub const Y: &[(f32, f32)] = &[#(#y_response),*];
                pub const Z: &[(f32, f32)] = &[#(#z_response),*];
            }
        }
        .to_string(),
    )?;

    println!("cargo:rerun-if-changed=build.rs");
    Ok(())
}

#[derive(Debug, Deserialize)]
struct XyzResponse {
    wavelength: f32,
    x: f32,
    y: f32,
    z: f32,
}
