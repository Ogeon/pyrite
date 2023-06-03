use std::error::Error;

use super::Nodes;

#[derive(Clone, Copy)]
pub(crate) struct EvalContext<'a> {
    pub nodes: &'a Nodes,
}

pub(crate) trait Evaluate<T> {
    fn evaluate<'a>(&self, context: EvalContext<'a>) -> Result<T, Box<dyn Error>>;
}

pub(crate) trait EvaluateOr<T> {
    fn evaluate_or<'a>(&self, context: EvalContext<'a>, default: T) -> Result<T, Box<dyn Error>>;

    fn evaluate_or_else<'a>(
        &self,
        context: EvalContext<'a>,
        default: impl FnOnce() -> T,
    ) -> Result<T, Box<dyn Error>>;
}

impl<T: Evaluate<U>, U> Evaluate<Option<U>> for Option<T> {
    fn evaluate<'a>(&self, context: EvalContext<'a>) -> Result<Option<U>, Box<dyn Error>> {
        self.as_ref().map(|v| v.evaluate(context)).transpose()
    }
}

impl<T: Evaluate<U>, U> EvaluateOr<U> for Option<T> {
    fn evaluate_or<'a>(&self, context: EvalContext<'a>, default: U) -> Result<U, Box<dyn Error>> {
        let maybe_value: Option<U> = self.evaluate(context)?;
        Ok(maybe_value.unwrap_or(default))
    }

    fn evaluate_or_else<'a>(
        &self,
        context: EvalContext<'a>,
        default: impl FnOnce() -> U,
    ) -> Result<U, Box<dyn Error>> {
        let maybe_value: Option<U> = self.evaluate(context)?;
        Ok(maybe_value.unwrap_or_else(default))
    }
}
