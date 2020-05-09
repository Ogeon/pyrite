use cgmath::{EuclideanSpace, InnerSpace, Point3, Quaternion, Vector3};

use config::entry::Entry;
use config::Prelude;

use tracer::ParametricValue;

use shapes::DistanceEstimator;

struct Mandelbulb {
    iterations: u16,
    threshold: f64,
    power: f64,
    constant: Option<Vector3<f64>>,
}

impl ParametricValue<Point3<f64>, f64> for Mandelbulb {
    fn get(&self, point: &Point3<f64>) -> f64 {
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

struct QuaternionJulia {
    iterations: u16,
    threshold: f64,
    constant: Quaternion<f64>,
    slice_plane: f64,
    ty: QuatMul,
}

impl ParametricValue<Point3<f64>, f64> for QuaternionJulia {
    fn get(&self, point: &Point3<f64>) -> f64 {
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

enum QuatMul {
    Regular,
    Cubic,
    Bicomplex,
}

impl QuatMul {
    fn pow(&self, z: &Quaternion<f64>) -> Quaternion<f64> {
        match *self {
            QuatMul::Regular => z * z,
            QuatMul::Cubic => z * z * z,
            QuatMul::Bicomplex => bicomplex_mul(z, z),
        }
    }

    fn pow_prim(&self, z: &Quaternion<f64>, dz: &Quaternion<f64>) -> Quaternion<f64> {
        match *self {
            QuatMul::Regular => dz * z * 2.0,
            QuatMul::Cubic => dz * z * z * 3.0,
            QuatMul::Bicomplex => bicomplex_mul(&bicomplex_mul(dz, z), z) * 2.0,
        }
    }
}

fn bicomplex_mul(a: &Quaternion<f64>, b: &Quaternion<f64>) -> Quaternion<f64> {
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

pub fn register_types(context: &mut Prelude) {
    {
        let mut group = context.object("RayMarched".into());
        group
            .object("Mandelbulb".into())
            .add_decoder(decode_mandelbulb);
        group
            .object("QuaternionJulia".into())
            .add_decoder(decode_quaternion_julia);
    }

    {
        let mut group = context.object("QuaternionJulia".into());
        group
            .object("Regular".into())
            .add_decoder(decode_quat_mul_regular);
        group
            .object("Cubic".into())
            .add_decoder(decode_quat_mul_cubic);
        group
            .object("Bicomplex".into())
            .add_decoder(decode_quat_mul_bicomplex);
    }
}

fn decode_mandelbulb(entry: Entry) -> Result<DistanceEstimator, String> {
    let items = try!(entry.as_object().ok_or("not an object".into()));

    let iterations = match items.get("iterations") {
        Some(v) => try!(v.decode(), "iterations"),
        None => return Err("missing field 'iterations'".into()),
    };

    let threshold = match items.get("threshold") {
        Some(v) => try!(v.decode(), "threshold"),
        None => return Err("missing field 'threshold'".into()),
    };

    let power = match items.get("power") {
        Some(v) => try!(v.decode(), "power"),
        None => return Err("missing field 'power'".into()),
    };

    let constant = match items.get("constant") {
        Some(v) => Some(try!(v.dynamic_decode(), "constant")),
        None => None,
    };

    Ok(Box::new(Mandelbulb {
        iterations: iterations,
        threshold: threshold,
        power: power,
        constant: constant,
    }))
}

fn decode_quaternion_julia(entry: Entry) -> Result<DistanceEstimator, String> {
    let items = try!(entry.as_object().ok_or("not an object".into()));

    let iterations = match items.get("iterations") {
        Some(v) => try!(v.decode(), "iterations"),
        None => return Err("missing field 'iterations'".into()),
    };

    let threshold = match items.get("threshold") {
        Some(v) => try!(v.decode(), "threshold"),
        None => return Err("missing field 'threshold'".into()),
    };

    let constant = match items.get("constant") {
        Some(v) => try!(v.dynamic_decode(), "constant"),
        None => return Err("missing field 'constant'".into()),
    };

    let slice_plane = match items.get("slice_plane") {
        Some(v) => try!(v.decode(), "slice_plane"),
        None => return Err("missing field 'slice_plane'".into()),
    };

    let ty = match items.get("type") {
        Some(v) => try!(v.dynamic_decode(), "type"),
        None => QuatMul::Regular,
    };

    Ok(Box::new(QuaternionJulia {
        iterations: iterations,
        threshold: threshold,
        constant: constant,
        slice_plane: slice_plane,
        ty: ty,
    }))
}

fn decode_quat_mul_regular(_entry: Entry) -> Result<QuatMul, String> {
    Ok(QuatMul::Regular)
}

fn decode_quat_mul_cubic(_entry: Entry) -> Result<QuatMul, String> {
    Ok(QuatMul::Cubic)
}

fn decode_quat_mul_bicomplex(_entry: Entry) -> Result<QuatMul, String> {
    Ok(QuatMul::Bicomplex)
}
