mod seal {
    use std::marker::PhantomData;

    pub(crate) trait Sealed {}

    pub(crate) type OnlyDAGNode = usize;
    pub(crate) type OnlyDAG<NodeData> = PhantomData<NodeData>;

    impl<NodeData> Sealed for OnlyDAG<NodeData> {}
}

pub type OnlyDAGNode = seal::OnlyDAGNode;
pub type OnlyDAG<NodeData> = seal::OnlyDAG<NodeData>;

pub(crate) trait DAGImplementor<NodeId, NodeData>: seal::Sealed {
    fn new() -> Self;

    fn peel_front(&mut self) -> impl Iterator<Item = NodeData>;

    fn is_empty(&self) -> bool;

    fn post_compose(&mut self, other: Self);

    #[allow(dead_code)]
    fn coproduct(&mut self, other: Self);
}

impl<NodeData> DAGImplementor<OnlyDAGNode, NodeData> for OnlyDAG<NodeData> {
    fn new() -> Self {
        Self {}
    }

    fn peel_front(&mut self) -> impl Iterator<Item = NodeData> {
        // TODO:
        vec![].into_iter()
    }

    fn is_empty(&self) -> bool {
        // TODO:
        true
    }

    fn post_compose(&mut self, _other: Self) {
        todo!()
    }

    fn coproduct(&mut self, _other: Self) {
        todo!()
    }
}
