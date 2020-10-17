use crate::{light::LightPool, program::ExecutionContext, renderer::samplers::Sampler};

pub(crate) struct Tools<'r, 'a> {
    pub sampler: &'r mut dyn Sampler,
    pub light_pool: &'r LightPool<'r>,
    pub execution_context: &'r mut ExecutionContext<'a>,
}
