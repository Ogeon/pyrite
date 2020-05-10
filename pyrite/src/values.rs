use tracer;

use config::Prelude;
use config::entry::Entry;

macro_rules! make_values {
    ($insert_fn:ident : $type_name:ident <$context_name:ty, $result_name:ty> { $($fn_name:ident : $variant_name:ident => $value_name:ident),* }) => (
        fn $insert_fn(context: &mut Prelude) {
            let mut group = context.object("Value".into());
            $(
                group.object(stringify!($variant_name).into()).add_decoder($fn_name);
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
                    $($type_name::$variant_name => i.$value_name),*
                }
            }
        }

        $(
            fn $fn_name(_entry: Entry) -> Result<Box<dyn tracer::ParametricValue<$context_name, $result_name>>, String> {
                Ok(Box::new($type_name::$variant_name) as Box<dyn tracer::ParametricValue<$context_name, $result_name>>)
            }
        )*
    )
}

pub fn register_types(context: &mut Prelude) {
    insert_render_numbers(context);
}

make_values! {
    insert_render_numbers: RenderNumber<tracer::RenderContext, f64> {
        decode_wavelength: Wavelength => wavelength
    }
}