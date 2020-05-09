pub mod bkd_tree;
pub mod kd_tree;

pub trait Dimensions: Copy {
    fn first() -> Self;
    fn next(&self) -> Self;
}

#[derive(Copy, Clone)]
pub enum Dim3 {
    X,
    Y,
    Z,
}

impl Dimensions for Dim3 {
    fn first() -> Dim3 {
        Dim3::X
    }

    fn next(&self) -> Dim3 {
        match *self {
            Dim3::X => Dim3::Y,
            Dim3::Y => Dim3::Z,
            Dim3::Z => Dim3::X
        }
    }
}