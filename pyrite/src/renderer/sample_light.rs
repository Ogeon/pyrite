use cgmath::{InnerSpace, Point2, Point3, Vector3};
use collision::Ray3;

use crate::{
    lamp::Lamp,
    light::{CoherentLight, Wavelengths},
    materials::{Material, MaterialInput},
    math::power_heuristic,
    shapes::Normal,
    tracer::NormalInput,
    utils::Tools,
    world::World,
};

pub(crate) fn sample_light<'t, 'a>(
    world: &'a World<'a>,
    hit: &Hit<'a>,
    wavelengths: &Wavelengths,
    tools: &mut Tools<'t, 'a>,
) -> Option<CoherentLight<'t>> {
    let lamps = &world.lights[..];
    let num_lamps = lamps.len();
    let lamp = tools.sampler.select(lamps)?;

    let mut light = estimate_direct(world, hit, lamp, wavelengths, tools);
    light *= num_lamps as f32;

    Some(light)
}

fn estimate_direct<'t, 'a>(
    world: &'a World<'a>,
    hit: &Hit<'a>,
    lamp: &Lamp<'a>,
    wavelengths: &Wavelengths,
    tools: &mut Tools<'t, 'a>,
) -> CoherentLight<'t> {
    let material_input = MaterialInput {
        normal: hit.normal.vector(),
        ray_direction: -hit.out_direction,
        texture_coordinate: hit.texture_coordinate,
    };

    let mut sample = lamp.sample_emission(world, hit.position, wavelengths, tools);

    let mut reflected_light = tools.light_pool.get();

    if sample.pdf > 0.0 && !sample.light.is_black() {
        let reflection = hit.bsdf.evaluate_coherent(
            hit.out_direction,
            hit.normal,
            sample.in_direction,
            hit.texture_coordinate,
            wavelengths,
            tools,
        ) * sample.in_direction.dot(hit.normal.vector()).abs();

        let scattering_pdf = hit.bsdf.pdf(
            hit.out_direction,
            hit.normal,
            sample.in_direction,
            &material_input,
            tools.execution_context,
        );

        if !reflection.is_black() {
            if !sample.visible {
                sample.light.set_all(0.0);
            }

            if !sample.light.is_black() {
                if matches!(lamp, &Lamp::Shape(_)) {
                    let weight = power_heuristic(1.0, sample.pdf, 1.0, scattering_pdf);
                    reflected_light += reflection * sample.light * weight / sample.pdf;
                } else {
                    reflected_light += reflection * sample.light / sample.pdf;
                }
            }
        }
    }

    // BSDF multiple importance sampling
    if let &Lamp::Shape(lamp_shape) = lamp {
        let interaction = hit
            .bsdf
            .sample_reflection_coherent(
                hit.out_direction,
                hit.texture_coordinate,
                hit.normal,
                wavelengths,
                tools,
            )
            .expect("the path should have terminated before direct light sampling");

        match interaction.output {
            crate::materials::InteractionOutput::Coherent(mut output) => {
                let scattering_pdf = output.pdf;

                output.reflectivity *= output.in_direction.dot(hit.normal.vector()).abs();

                if !output.reflectivity.is_black() && scattering_pdf > 0.0 {
                    let weight = if interaction.diffuse {
                        let light_pdf = lamp_shape.emission_pdf(
                            hit.position,
                            output.in_direction,
                            sample.normal,
                        );
                        if light_pdf == 0.0 {
                            return reflected_light;
                        }

                        power_heuristic(1.0, scattering_pdf, 1.0, light_pdf)
                    } else {
                        1.0
                    };

                    let ray = Ray3::new(hit.position, output.in_direction);
                    let intersection = world.intersect(ray);

                    let light_contribution = if let Some(intersection) = intersection {
                        if intersection.surface_point.is_shape(lamp_shape) {
                            let surface_data = intersection.surface_point.get_surface_data();
                            let material = intersection.surface_point.get_material();

                            let input = NormalInput {
                                normal: surface_data.normal.vector(),
                                incident: ray.direction,
                                texture: surface_data.texture,
                            };
                            let shading_normal = material.apply_normal_map(
                                surface_data.normal,
                                input,
                                tools.execution_context,
                            );

                            material.light_emission(
                                -output.in_direction,
                                shading_normal,
                                surface_data.texture,
                                wavelengths,
                                tools,
                            )
                        } else {
                            None
                        }
                    } else {
                        // Some(sample.light_emission(ray))
                        None
                    };

                    if let Some(light_contribution) = light_contribution {
                        if !light_contribution.is_black() {
                            reflected_light +=
                                output.reflectivity * light_contribution * weight / scattering_pdf;
                        }
                    }
                }
            }
            crate::materials::InteractionOutput::Dispersed(_) => {
                unimplemented!()
            }
        }
    }

    reflected_light
}

pub(crate) struct Hit<'a> {
    pub position: Point3<f32>,
    pub out_direction: Vector3<f32>,
    pub normal: Normal,
    pub texture_coordinate: Point2<f32>,
    pub bsdf: Material<'a>,
}
