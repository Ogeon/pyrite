use std::collections::HashMap;

use tracer;

use config;

macro_rules! make_values(
    ($insert_fn:ident : $type_name:ident <$context_name:ty, $result_name:ty> { $($fn_name:ident : $variant_name:ident => $value_name:ident),* }) => (
        fn $insert_fn(context: &mut config::ConfigContext) {
            $(
                context.insert_grouped_type("Value", stringify!($variant_name), $fn_name);
            )*
        }

        enum $type_name {
            $(
               $variant_name
            ),*
        }

        impl tracer::ParametricValue<$context_name, $result_name> for $type_name {
            fn get(&self, i: &$context_name) -> $result_name {
                match *self {
                	$($variant_name => i.$value_name),*
                }
            }
        }

        $(
	        fn $fn_name(_context: &config::ConfigContext, _fields: HashMap<String, config::ConfigItem>) -> Result<Box<tracer::ParametricValue<$context_name, $result_name> + 'static + Send + Sync>, String> {
	            Ok(box $variant_name as Box<tracer::ParametricValue<$context_name, $result_name> + 'static + Send + Sync>)
	        }
        )*
    )
)

pub fn register_types(context: &mut config::ConfigContext) {
    insert_render_numbers(context);
}

make_values! {
	insert_render_numbers: RenderNumber<tracer::RenderContext, f64> {
		decode_frequency: Frequency => frequency
	}
}