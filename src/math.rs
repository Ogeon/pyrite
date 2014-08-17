use std::collections::HashMap;

use cgmath::Vector;

use tracer;

use config;
use config::FromConfig;

macro_rules! make_operators(
    ($($fn_name:ident : $struct_name:ident { $($arg:ident),+ } => $operation:expr),*) => (
        fn insert_operators<From>(context: &mut config::ConfigContext) {
            $(
                context.insert_grouped_type("Math", stringify!($struct_name), $fn_name::<From>);
            )*
        }
        $(

            struct $struct_name<From> {
                $(
                    $arg: Box<tracer::ParametricValue<From, f64> + 'static + Send + Sync>
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

            fn $fn_name<From>(context: &config::ConfigContext, fields: HashMap<String, config::ConfigItem>) -> Result<Box<tracer::ParametricValue<From, f64> + 'static + Send + Sync>, String> {
                let mut fields = fields;

                $(
                    let $arg = match fields.pop_equiv(&stringify!($arg)) {
                        Some(v) => try!(tracer::decode_parametric_number(context, v), stringify!($arg)),
                        None => return Err(format!("missing field '{}'", stringify!($arg)))
                    };
                )+

                Ok(
                    box $struct_name::<From> {
                        $(
                            $arg: $arg
                        ),+
                    } as Box<tracer::ParametricValue<From, f64> + 'static + Send + Sync>
                )
            }

        )*
    )
)

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

            let (min_x, min_y) = self.points[min];

            if min_x >= input {
                return 0.0 // min_y
            }

            let (max_x, max_y) = self.points[max];

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
            let sinT2 = n * n * (1.0 - cos_psi * cos_psi);
            if sinT2 > 1.0 {
                return 1.0;
            }
            cos_psi = (1.0 - sinT2).sqrt();
        }

        let inv_cos = 1.0 - cos_psi;

        return r0 * r0 + (1.0 - r0 * r0) * inv_cos * inv_cos * inv_cos * inv_cos * inv_cos;
    }
}

pub fn register_types<From>(context: &mut config::ConfigContext) {
    insert_operators::<From>(context);
    context.insert_grouped_type("Math", "Curve", decode_curve::<From>);
}

pub fn register_specific_types(context: &mut config::ConfigContext) {
    context.insert_grouped_type("Math", "Fresnel", decode_fresnel);
}

make_operators!{
    decode_add: Add { a, b }         => a + b,
    decode_sub: Sub { a, b }         => a - b,
    decode_mul: Mul { a, b }         => a * b,
    decode_div: Div { a, b }         => a / b,
    decode_abs: Abs { a }            => a.abs(),
    decode_min: Min { a, b }         => a.min(b),
    decode_max: Max { a, b }         => a.max(b),
    decode_mix: Mix { factor, a, b } => { let f = factor.min(1.0).max(0.0); a * (1.0 - f) + b * f }
}

struct Curve<From> {
    input: Box<tracer::ParametricValue<From, f64> + 'static + Send + Sync>,
    points: utils::Interpolated
}

impl<From> tracer::ParametricValue<From, f64> for Curve<From> {
    fn get(&self, i: &From) -> f64 {
        self.points.get(self.input.get(i))
    }
}

fn decode_curve<From>(context: &config::ConfigContext, fields: HashMap<String, config::ConfigItem>) -> Result<Box<tracer::ParametricValue<From, f64> + 'static + Send + Sync>, String> {
    let mut fields = fields;

    let input = match fields.pop_equiv(&"input") {
        Some(v) => try!(tracer::decode_parametric_number(context, v), "input"),
        None => return Err(String::from_str("missing field 'input'"))
    };

    let points = match fields.pop_equiv(&"points") {
        Some(v) => try!(FromConfig::from_config(v), "points"),
        None => return Err(String::from_str("missing field 'points'"))
    };

    Ok(
        box Curve::<From> {
            input: input,
            points: utils::Interpolated { points: points }
        } as Box<tracer::ParametricValue<From, f64> + 'static + Send + Sync>
    )
}


struct Fresnel {
    ior: Box<tracer::ParametricValue<tracer::RenderContext, f64> + 'static + Send + Sync>,
    env_ior: Box<tracer::ParametricValue<tracer::RenderContext, f64> + 'static + Send + Sync>
}

impl tracer::ParametricValue<tracer::RenderContext, f64> for Fresnel {
    fn get(&self, i: &tracer::RenderContext) -> f64 {
        let normal = &i.normal;
        let incident = &i.incident;

        if incident.dot(normal) < 0.0 {
            utils::schlick(self.env_ior.get(i), self.ior.get(i), normal, incident)
        } else {
            utils::schlick(self.ior.get(i), self.env_ior.get(i), &-normal, incident)
        }
    }
}

fn decode_fresnel(context: &config::ConfigContext, fields: HashMap<String, config::ConfigItem>) -> Result<Box<tracer::ParametricValue<tracer::RenderContext, f64> + 'static + Send + Sync>, String> {
    let mut fields = fields;

    let ior = match fields.pop_equiv(&"ior") {
        Some(v) => try!(tracer::decode_parametric_number(context, v), "ior"),
        None => return Err(String::from_str("missing field 'ior'"))
    };

    let env_ior = match fields.pop_equiv(&"env_ior") {
        Some(v) => try!(tracer::decode_parametric_number(context, v), "env_ior"),
        None => box 1.0f64 as Box<tracer::ParametricValue<tracer::RenderContext, f64> + 'static + Send + Sync>
    };

    Ok(
        box Fresnel {
            ior: ior,
            env_ior: env_ior
        } as Box<tracer::ParametricValue<tracer::RenderContext, f64> + 'static + Send + Sync>
    )
}