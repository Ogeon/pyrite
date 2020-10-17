use std::{error::Error, f32::consts};

use cgmath::{Angle, EuclideanSpace, InnerSpace, Matrix4, Point2, Point3, Rad, Vector2};
use collision::{Ray, Ray3};

use crate::{
    film::Area,
    project::{
        eval_context::{EvalContext, Evaluate, EvaluateOr},
        expressions::Expressions,
    },
    renderer::samplers::Sampler,
};

pub(crate) enum Camera {
    Perspective {
        transform: Matrix4<f32>,
        view_plane: f32,
        focus_distance: f32,
        aperture: f32,
    },
}

impl Camera {
    pub fn from_project(
        project_camera: crate::project::Camera,
        expressions: &Expressions,
    ) -> Result<Self, Box<dyn Error>> {
        let eval_context = EvalContext { expressions };

        match project_camera {
            crate::project::Camera::Perspective {
                transform,
                fov,
                focus_distance,
                aperture,
            } => {
                let fov: f32 = fov.evaluate(eval_context)?;
                let fov_radians: Rad<_> = cgmath::Deg(fov * 0.5f32).into();
                let view_plane = fov_radians.cos() / fov_radians.sin();

                Ok(Camera::Perspective {
                    transform: transform.evaluate(eval_context)?,
                    view_plane,
                    focus_distance: focus_distance.evaluate_or(eval_context, 1.0)?,
                    aperture: aperture.evaluate_or(eval_context, 0.0)?,
                })
            }
        }
    }

    pub fn to_view_area(&self, area: &Area<usize>, width: usize, height: usize) -> Area<f32> {
        let float_image_size = Vector2::new(width as f32, height as f32);
        let float_coord = Point2::new(area.from.x as f32, area.from.y as f32);
        let float_size = Vector2::new(area.size.x as f32, area.size.y as f32);

        let max_dimension = float_image_size.x.max(float_image_size.y);

        let from = (float_coord + (-float_image_size * 0.5)) / (max_dimension * 0.5);
        let size = float_size / (max_dimension * 0.5);

        Area::new(from, size)
    }

    pub fn ray_towards(&self, target: Point2<f32>, rng: &mut dyn Sampler) -> Ray3<f32> {
        match *self {
            Camera::Perspective {
                transform,
                view_plane,
                focus_distance,
                aperture,
            } => {
                let focus_x = target.x / view_plane * focus_distance;
                let focus_y = target.y / view_plane * focus_distance;

                let target = Point3::new(focus_x, -focus_y, -focus_distance);

                let (origin, direction) = if aperture > 0.0 {
                    let sqrt_r = (aperture * rng.gen::<f32>()).sqrt();
                    let psi = consts::PI * 2.0 * rng.gen::<f32>();
                    let lens_x = sqrt_r * psi.cos();
                    let lens_y = sqrt_r * psi.sin();
                    let origin = Point3::new(lens_x, lens_y, 0.0);
                    (origin, target - origin)
                } else {
                    (Point3::origin(), target.to_vec())
                };

                Ray::new(origin, direction.normalize()).transform(transform)
            }
        }
    }
}
