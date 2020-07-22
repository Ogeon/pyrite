use cgmath::Point3;

pub(crate) mod bvh;
pub(crate) mod kd_tree;

pub trait Dimensions: Copy {
    fn first() -> Self;
    fn next(&self) -> Self;
}

#[derive(Copy, Clone, Debug)]
pub enum Dim3 {
    X,
    Y,
    Z,
}

impl Dim3 {
    fn point_element(&self, point: Point3<f32>) -> f32 {
        match self {
            Dim3::X => point.x,
            Dim3::Y => point.y,
            Dim3::Z => point.z,
        }
    }
}

impl Dimensions for Dim3 {
    fn first() -> Dim3 {
        Dim3::X
    }

    fn next(&self) -> Dim3 {
        match *self {
            Dim3::X => Dim3::Y,
            Dim3::Y => Dim3::Z,
            Dim3::Z => Dim3::X,
        }
    }
}
