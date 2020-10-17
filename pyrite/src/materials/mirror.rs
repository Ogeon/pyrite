use cgmath::{Point2, Vector3};

use crate::{light::Wavelengths, tracer::LightProgram, tracer::RenderContext, utils::Tools};

use super::SurfaceInteraction;

pub(super) fn sample_reflection<'t, 'a>(
    out_direction: Vector3<f32>,
    texture_coordinate: Point2<f32>,
    color: LightProgram<'a>,
    wavelengths: &Wavelengths,
    tools: &mut Tools<'t, 'a>,
) -> SurfaceInteraction<'t> {
    let in_direction = Vector3::new(-out_direction.x, -out_direction.y, out_direction.z);

    let mut reflectivity = tools.light_pool.get();

    if in_direction.z != 0.0 {
        let initial_input = RenderContext {
            wavelength: wavelengths.hero(),
            normal: Vector3::unit_z(),
            ray_direction: -out_direction,
            texture: texture_coordinate,
        };

        let mut color_program = color.memoize(initial_input, tools.execution_context);

        let abs_cos_in = in_direction.z.abs();

        if abs_cos_in > 0.0 {
            for (bin, wavelength) in reflectivity.iter_mut().zip(wavelengths) {
                color_program.update_input().set_wavelength(wavelength);
                *bin = color_program.run() / abs_cos_in;
            }
        }
    }

    SurfaceInteraction {
        in_direction,
        pdf: if in_direction.z == 0.0 { 0.0 } else { 1.0 },
        diffuse: false,
        reflectivity,
    }
}
