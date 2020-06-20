use cgmath::{EuclideanSpace, InnerSpace, Point3, Quaternion, Vector3};

use crate::tracer::ParametricValue;

pub struct Mandelbulb {
    pub iterations: u16,
    pub threshold: f32,
    pub power: f32,
    pub constant: Option<Vector3<f32>>,
}

impl ParametricValue<Point3<f32>, f32> for Mandelbulb {
    fn get(&self, point: &Point3<f32>) -> f32 {
        let mut z = point.to_vec();
        let mut r = 0.0;
        let mut dr = 1.0;
        let dc = if self.constant.is_none() { 1.0 } else { 0.0 };

        for _ in 0..self.iterations {
            r = z.magnitude();
            if r > self.threshold {
                break;
            }

            let mut theta = (z.z / r).acos();
            let mut phi = z.y.atan2(z.x);
            dr = r.powf(self.power - 1.0) * self.power * dr + dc;

            let zr = r.powf(self.power);
            theta *= self.power;
            phi *= self.power;
            z = Vector3::new(
                zr * theta.sin() * phi.cos(),
                zr * phi.sin() * theta.sin(),
                zr * theta.cos(),
            );
            z += self.constant.unwrap_or(point.to_vec());
        }

        0.5 * r.ln() * r / dr
    }
}

pub struct QuaternionJulia {
    pub iterations: u16,
    pub threshold: f32,
    pub constant: Quaternion<f32>,
    pub slice_plane: f32,
    pub ty: QuatMul,
}

impl ParametricValue<Point3<f32>, f32> for QuaternionJulia {
    fn get(&self, point: &Point3<f32>) -> f32 {
        let mut z = Quaternion::new(point.x, point.y, point.z, self.slice_plane);
        let mut r = 0.0;
        let mut dz = Quaternion::new(1.0, 0.0, 0.0, 0.0);

        for _ in 0..self.iterations {
            r = z.magnitude();
            if r > self.threshold {
                break;
            }

            dz = self.ty.pow_prim(&z, &dz);
            z = self.ty.pow(&z) + self.constant;
        }

        0.5 * r.ln() * r / dz.magnitude()
    }
}

pub enum QuatMul {
    Regular,
    Cubic,
    Bicomplex,
}

impl QuatMul {
    fn pow(&self, z: &Quaternion<f32>) -> Quaternion<f32> {
        match *self {
            QuatMul::Regular => z * z,
            QuatMul::Cubic => z * z * z,
            QuatMul::Bicomplex => bicomplex_mul(z, z),
        }
    }

    fn pow_prim(&self, z: &Quaternion<f32>, dz: &Quaternion<f32>) -> Quaternion<f32> {
        match *self {
            QuatMul::Regular => dz * z * 2.0,
            QuatMul::Cubic => dz * z * z * 3.0,
            QuatMul::Bicomplex => bicomplex_mul(&bicomplex_mul(dz, z), z) * 2.0,
        }
    }
}

fn bicomplex_mul(a: &Quaternion<f32>, b: &Quaternion<f32>) -> Quaternion<f32> {
    let (x1, x2) = (a.s, b.s);
    let (y1, y2) = (a.v.x, b.v.x);
    let (z1, z2) = (a.v.y, b.v.y);
    let (w1, w2) = (a.v.z, b.v.z);

    let x = x1 * x2 - y1 * y2 - z1 * z2 + w1 * w2;
    let y = x1 * y2 + y1 * x2 - z1 * w2 - w1 * z2;
    let z = x1 * z2 - y1 * w2 + z1 * x2 - w1 * y2;
    let w = x1 * w2 + y1 * z2 + z1 * y2 + w1 * x2;
    Quaternion::new(x, y, z, w)
}
