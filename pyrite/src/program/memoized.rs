use super::{execution_context::MemoizedContext, ExecutionContext, Inputs, Program, ProgramInput};

pub(crate) struct MemoizedProgram<'a, 'p, I: ProgramInput, T> {
    input: I,
    changes: Inputs,
    context: MemoizedContext<'a, 'p, I, T>,
}

impl<'a, 'p, I: ProgramInput, T> MemoizedProgram<'a, 'p, I, T> {
    pub fn new(
        program: Program<'p, I::NumberInput, I::VectorInput, T>,
        initial_input: I,
        context: &'a mut ExecutionContext<'p>,
    ) -> Self {
        MemoizedProgram {
            input: initial_input,
            changes: Inputs::all(),
            context: context.memoized(program),
        }
    }

    pub fn update_input<'r>(&'r mut self) -> I::Updater
    where
        I: MemoizedInput<'r>,
    {
        self.input.new_updater(&mut self.changes)
    }

    pub fn run(&mut self) -> T {
        let result = self.context.run(&self.input, self.changes);
        self.changes = Inputs::empty();
        result
    }
}

pub(crate) trait MemoizedInput<'r> {
    type Updater;

    fn new_updater(&'r mut self, changes: &'r mut Inputs) -> Self::Updater;
}
