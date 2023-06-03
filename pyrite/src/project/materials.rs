use typed_nodes::Key;

use super::expressions::Expression;

#[derive(typed_nodes::FromLua)]
#[typed_nodes(is_node)]
pub(crate) enum SurfaceMaterial {
    Emissive {
        color: Expression,
    },
    Diffuse {
        color: Expression,
    },
    Mirror {
        color: Expression,
    },
    Refractive {
        color: Expression,
        ior: Expression,
        dispersion: Option<Expression>,
        env_ior: Option<Expression>,
        env_dispersion: Option<Expression>,
    },
    Mix {
        #[typed_nodes(recursive)]
        lhs: Key<SurfaceMaterial>,
        rhs: Key<SurfaceMaterial>,
        amount: Expression,
    },
    Binary {
        operator: BinaryOperator,
        lhs: Key<SurfaceMaterial>,
        rhs: Key<SurfaceMaterial>,
    },
}

#[derive(Copy, Clone, typed_nodes::FromLua)]
pub enum BinaryOperator {
    Add,
}

#[derive(Debug, Copy, Clone, Ord, PartialOrd, Eq, PartialEq, Hash)]
#[repr(transparent)]
pub(crate) struct MaterialId(usize);
