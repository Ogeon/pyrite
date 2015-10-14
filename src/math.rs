use cgmath::Vector;

use tracer;

use config::{Prelude, Decode};
use config::entry::Entry;

macro_rules! make_operators {
    ($($fn_name:ident : $struct_name:ident { $($arg:ident),+ } => $operation:expr),*) => (
        fn insert_operators<From: Decode + 'static>(context: &mut Prelude) {
            let mut group = context.object("Math".into());
            $(
                {
                    let mut object = group.object(stringify!($struct_name).into());
                    object.add_decoder($fn_name::<From>);
                    object.arguments(vec![$(stringify!($arg).into()),+]);
                }
            )*
        }
        $(

            struct $struct_name<From> {
                $(
                    $arg: Box<tracer::ParametricValue<From, f64>>
                ),+
            }

            impl<From> tracer::ParametricValue<From, f64> for $struct_name<From> {
                fn get(&self, i: &From) -> f64 {
                    $(
                        let $arg = self.$arg.get(i);
                    )+
                    $operation
                }
            }

            fn $fn_name<From: Decode + 'static>(entry: Entry) -> Result<Box<tracer::ParametricValue<From, f64>>, String> {
                let fields = try!(entry.as_object().ok_or("not an object".into()));

                $(
                    let $arg = match fields.get(stringify!($arg)) {
                        Some(v) => try!(tracer::decode_parametric_number(v), stringify!($arg)),
                        None => return Err(format!("missing field '{}'", stringify!($arg)))
                    };
                )+

                Ok(
                    Box::new($struct_name::<From> {
                        $(
                            $arg: $arg
                        ),+
                    }) as Box<tracer::ParametricValue<From, f64>>
                )
            }

        )*
    )
}

pub mod utils {
    use cgmath::{Vector, Vector3};

    pub struct Interpolated {
        pub points: Vec<(f64, f64)>
    }

    impl Interpolated {
        pub fn get(&self, input: f64) -> f64 {
            if self.points.len() == 0 {
                return 0.0
            }

            let mut min = 0;
            let mut max = self.points.len() - 1;

            let (min_x, _min_y) = self.points[min];

            if min_x >= input {
                return 0.0 // min_y
            }

            let (max_x, _max_y) = self.points[max];

            if max_x <= input {
                return 0.0 // max_y
            }


            while max > min + 1 {
                let check = (max + min) / 2;
                let (check_x, check_y) = self.points[check];

                if check_x == input {
                    return check_y
                }

                if check_x > input {
                    max = check
                } else {
                    min = check
                }
            }

            let (min_x, min_y) = self.points[min];
            let (max_x, max_y) = self.points[max];

            if input < min_x {
                0.0 //min_y
            } else if input > max_x {
                0.0 //max_y
            } else {
                min_y + (max_y - min_y) * (input - min_x) / (max_x - min_x)
            }
        }
    }

    pub fn schlick(ref_index1: f64, ref_index2: f64, normal: &Vector3<f64>, incident: &Vector3<f64>) -> f64 {
        let mut cos_psi = -normal.dot(incident);
        let r0 = (ref_index1 - ref_index2) / (ref_index1 + ref_index2);

        if ref_index1 > ref_index2 {
            let n = ref_index1 / ref_index2;
            let sin_t2 = n * n * (1.0 - cos_psi * cos_psi);
            if sin_t2 > 1.0 {
                return 1.0;
            }
            cos_psi = (1.0 - sin_t2).sqrt();
        }

        let inv_cos = 1.0 - cos_psi;

        return r0 * r0 + (1.0 - r0 * r0) * inv_cos * inv_cos * inv_cos * inv_cos * inv_cos;
    }
}

pub fn register_types<From: Decode + 'static>(context: &mut Prelude) {
    insert_operators::<From>(context);
    context.object("Math".into()).object("Curve".into()).add_decoder(decode_curve::<From>);
}

pub fn register_specific_types(context: &mut Prelude) {
    let mut group = context.object("Math".into());
    let mut object = group.object("Fresnel".into());
    object.add_decoder(decode_fresnel);
    object.arguments(vec!["ior".into(), "env_ior".into()]);
}

make_operators!{
    decode_add: Add { a, b }         => a + b,
    decode_sub: Sub { a, b }         => a - b,
    decode_mul: Mul { a, b }         => a * b,
    decode_div: Div { a, b }         => a / b,
    decode_abs: Abs { a }            => a.abs(),
    decode_min: Min { a, b }         => a.min(b),
    decode_max: Max { a, b }         => a.max(b),
    decode_mix: Mix { a, b, factor } => { let f = factor.min(1.0).max(0.0); a * (1.0 - f) + b * f }
}

struct Curve<From> {
    input: Box<tracer::ParametricValue<From, f64>>,
    points: utils::Interpolated
}

impl<From> tracer::ParametricValue<From, f64> for Curve<From> {
    fn get(&self, i: &From) -> f64 {
        self.points.get(self.input.get(i))
    }
}

fn decode_curve<From: Decode + 'static>(entry: Entry) -> Result<Box<tracer::ParametricValue<From, f64>>, String> {
    let fields = try!(entry.as_object().ok_or("not an object".into()));

    let input = match fields.get("input") {
        Some(v) => try!(tracer::decode_parametric_number(v), "input"),
        None => return Err("missing field 'input'".into())
    };

    let points = match fields.get("points") {
        Some(v) => try!(v.decode(), "points"),
        None => return Err("missing field 'points'".into())
    };

    Ok(
        Box::new(Curve::<From> {
            input: input,
            points: utils::Interpolated { points: points }
        }) as Box<tracer::ParametricValue<From, f64>>
    )
}


struct Fresnel {
    ior: Box<tracer::ParametricValue<tracer::RenderContext, f64>>,
    env_ior: Box<tracer::ParametricValue<tracer::RenderContext, f64>>
}

impl tracer::ParametricValue<tracer::RenderContext, f64> for Fresnel {
    fn get(&self, i: &tracer::RenderContext) -> f64 {
        let normal = &i.normal;
        let incident = &i.incident;

        if incident.dot(normal) < 0.0 {
            utils::schlick(self.env_ior.get(i), self.ior.get(i), normal, incident)
        } else {
            utils::schlick(self.ior.get(i), self.env_ior.get(i), &-*normal, incident)
        }
    }
}

fn decode_fresnel(entry: Entry) -> Result<Box<tracer::ParametricValue<tracer::RenderContext, f64>>, String> {
    let fields = try!(entry.as_object().ok_or("not an object".into()));

    let ior = match fields.get("ior") {
        Some(v) => try!(tracer::decode_parametric_number(v), "ior"),
        None => return Err("missing field 'ior'".into())
    };

    let env_ior = match fields.get("env_ior") {
        Some(v) => try!(tracer::decode_parametric_number(v), "env_ior"),
        None => Box::new(1.0f64) as Box<tracer::ParametricValue<tracer::RenderContext, f64>>
    };

    Ok(
        Box::new(Fresnel {
            ior: ior,
            env_ior: env_ior
        })
    )
}