use std::collections::HashMap;

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

pub fn register_types<From>(context: &mut config::ConfigContext) {
    insert_operators::<From>(context);
    context.insert_grouped_type("Math", "Curve", decode_curve::<From>);
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
    points: Vec<(f64, f64)>
}

impl<From> tracer::ParametricValue<From, f64> for Curve<From> {
    fn get(&self, i: &From) -> f64 {
        let input = self.input.get(i);
        let mut points = self.points.iter();

        let mut min = match points.next() {
            Some(v) => *v,
            None => return 0.0
        };

        let mut max = min;

        for &(x, y) in points {
            if input == x {
                return y;
            } else if input > x {
                min = max;
                max = (x, y);
            } else {
                break;
            }
        }

        let (min_x, min_y) = min;
        let (max_x, max_y) = max;

        if input < min_x {
            min_y
        } else if input > max_x {
            max_y
        } else {
            min_y + (max_y - min_y) * (input - min_x) / (max_x - min_x)
        }
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
            points: points
        } as Box<tracer::ParametricValue<From, f64> + 'static + Send + Sync>
    )
}