use std::collections::HashMap;

use tracer;

use config;

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
    insert_operators::<From>(context)
}

make_operators!{
    decode_add: Add { a, b }         => a + b,
    decode_sub: Sub { a, b }         => a - b,
    decode_mul: Mul { a, b }         => a * b,
    decode_div: Div { a, b }         => a / b,
    decode_abs: Abs { a }            => a.abs(),
    decode_min: Min { a, b }         => a.min(b),
    decode_max: Max { a, b }         => a.max(b),
    decode_mix: Mix { factor, a, b } => a * (1.0 - factor) + b * factor
}