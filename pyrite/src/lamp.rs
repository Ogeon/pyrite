use std::unimplemented;

use cgmath::{InnerSpace, Point2, Point3, Vector3};
use collision::Ray3;

use crate::{
    light::{CoherentLight, Wavelengths},
    math::utils::{cosine_hemisphere_pdf, sample_cosine_hemisphere, sample_sphere},
    shapes::{Intersection, Shape},
    tracer::{LightProgram, NormalInput, RenderContext},
    utils::Tools,
    world::World,
};

pub(crate) enum Lamp<'p> {
    Directional {
        direction: Vector3<f32>,
        width: f32,
        color: LightProgram<'p>,
    },
    Point(Point3<f32>, LightProgram<'p>),
    Shape(&'p Shape<'p>),
}

impl<'p> Lamp<'p> {
    pub fn sample_emission<'t>(
        &self,
        world: &World<'p>,
        target: Point3<f32>,
        wavelengths: &Wavelengths,
        tools: &mut Tools<'t, 'p>,
    ) -> LightSample<'t> {
        match *self {
            Lamp::Directional {
                direction,
                width,
                color,
            } => {
                let in_direction = -direction;
                let visibility_ray = Ray3::new(target, in_direction);
                let visibility_intersection = world.intersect(visibility_ray);

                let mut light = tools.light_pool.get();

                let initial_input = RenderContext {
                    wavelength: wavelengths.hero(),
                    normal: Vector3::unit_z(),
                    ray_direction: in_direction,
                    texture: Point2::new(0.0, 0.0),
                };

                let mut color_program = color.memoize(initial_input, tools.execution_context);

                for (bin, wavelength) in light.iter_mut().zip(wavelengths) {
                    color_program.update_input().set_wavelength(wavelength);
                    *bin = color_program.run();
                }

                LightSample {
                    light,
                    pdf: 1.0,
                    in_direction,
                    normal: -in_direction,
                    visible: visibility_intersection.is_none(),
                }
            }
            Lamp::Point(center, color) => {
                let v = center - target;
                let distance = v.magnitude();

                let in_direction = v / distance;
                let visibility_ray = Ray3::new(target, in_direction);
                let visibility_intersection = world.intersect(visibility_ray);

                let mut light = tools.light_pool.get();

                let initial_input = RenderContext {
                    wavelength: wavelengths.hero(),
                    normal: Vector3::unit_z(),
                    ray_direction: in_direction,
                    texture: Point2::new(0.0, 0.0),
                };

                let mut color_program = color.memoize(initial_input, tools.execution_context);

                for (bin, wavelength) in light.iter_mut().zip(wavelengths) {
                    color_program.update_input().set_wavelength(wavelength);
                    *bin = color_program.run();
                }

                LightSample {
                    light,
                    pdf: 1.0,
                    in_direction,
                    normal: -in_direction,
                    visible: visibility_intersection
                        .map(|intersection| intersection.distance >= distance)
                        .unwrap_or(true),
                }
            }
            Lamp::Shape(shape) => {
                let Intersection { surface_point, .. } = shape
                    .sample_towards(tools.sampler, &target)
                    .expect("trying to use infinite shape in direct lighting");

                let in_direction = (surface_point.position - target).normalize();
                let visibility_ray = Ray3::new(target, in_direction);
                let visibility_intersection = world.intersect(visibility_ray);

                let surface_data = surface_point.get_surface_data();
                let material = surface_point.get_material();

                let normal_input = NormalInput {
                    normal: surface_data.normal.vector(),
                    incident: -in_direction,
                    texture: surface_data.texture,
                };
                let normal = material.apply_normal_map(
                    surface_data.normal,
                    normal_input,
                    tools.execution_context,
                );

                let light = surface_point
                    .get_material()
                    .light_emission(
                        -in_direction,
                        normal,
                        surface_data.texture,
                        wavelengths,
                        tools,
                    )
                    .expect("lamps should have emissive materials");

                let pdf = shape.emission_pdf(target, in_direction, normal);

                LightSample {
                    light,
                    pdf,
                    in_direction,
                    visible: visibility_intersection
                        .map(|intersection| intersection.surface_point.is_shape(shape))
                        .unwrap_or(false),
                    normal,
                }
            }
        }
    }

    pub(crate) fn sample_emission_out<'t>(
        &self,
        wavelengths: &Wavelengths,
        tools: &mut Tools<'t, 'p>,
    ) -> LightSampleOut<'t> {
        match *self {
            Lamp::Directional {
                direction,
                width,
                color,
            } => unimplemented!(),
            Lamp::Point(center, color) => {
                let ray = Ray3::new(center, sample_sphere(tools.sampler));
                let mut light = tools.light_pool.get();

                let initial_input = RenderContext {
                    wavelength: wavelengths.hero(),
                    normal: Vector3::unit_z(),
                    ray_direction: -ray.direction,
                    texture: Point2::new(0.0, 0.0),
                };

                let mut color_program = color.memoize(initial_input, tools.execution_context);

                for (bin, wavelength) in light.iter_mut().zip(wavelengths) {
                    color_program.update_input().set_wavelength(wavelength);
                    *bin = color_program.run();
                }

                LightSampleOut {
                    light,
                    pdf_pos: 1.0,
                    pdf_dir: crate::math::SPHERE_PDF,
                    ray,
                    normal: ray.direction,
                }
            }
            Lamp::Shape(shape) => {
                let surface_point = shape
                    .sample_point(tools.sampler)
                    .expect("trying to sample infinite surface light");
                let surface_data = surface_point.get_surface_data();
                let material = surface_point.get_material();

                let normal_input = NormalInput {
                    normal: surface_data.normal.vector(),
                    incident: surface_data.normal.vector(),
                    texture: surface_data.texture,
                };
                let normal = material.apply_normal_map(
                    surface_data.normal,
                    normal_input,
                    tools.execution_context,
                );

                let normal_space_direction = sample_cosine_hemisphere(tools.sampler);
                let ray = Ray3::new(
                    surface_point.position,
                    surface_data
                        .normal
                        .tilted(normal)
                        .from_space(normal_space_direction),
                );

                let light = surface_point
                    .get_material()
                    .light_emission(
                        ray.direction,
                        normal,
                        surface_data.texture,
                        wavelengths,
                        tools,
                    )
                    .expect("lamps should have emissive materials");

                LightSampleOut {
                    light,
                    pdf_pos: 1.0 / surface_point.get_surface_area(),
                    pdf_dir: cosine_hemisphere_pdf(normal_space_direction.z),
                    ray,
                    normal: surface_data.normal.vector(),
                }
            }
        }
    }
}

pub(crate) struct LightSample<'a> {
    pub light: CoherentLight<'a>,
    pub pdf: f32,
    pub in_direction: Vector3<f32>,
    pub normal: Vector3<f32>,
    pub visible: bool,
}

pub(crate) struct LightSampleOut<'a> {
    pub light: CoherentLight<'a>,
    pub pdf_pos: f32,
    pub pdf_dir: f32,
    pub ray: Ray3<f32>,
    pub normal: Vector3<f32>,
}
