use std::collections::HashMap;

use petgraph::{algo::Cycle, graph::NodeIndex, prelude::StableDiGraph, Direction};

mod seal {

    use petgraph::{stable_graph::StableDiGraph, visit::GraphBase};

    pub trait Sealed {}

    pub type MyDAGNode = <StableDiGraph<usize, ()> as GraphBase>::NodeId;
    pub struct MyDAG<NodeData, EdgeData> {
        pub(crate) underlying: StableDiGraph<NodeData, EdgeData>,
        pub(crate) dummy_node_data: fn(&NodeData) -> NodeData,
    }

    impl<NodeData, EdgeData> Sealed for MyDAG<NodeData, EdgeData> {}
}

pub type MyDAGNode = seal::MyDAGNode;
pub type MyDAG<NodeData, EdgeData> = seal::MyDAG<NodeData, EdgeData>;

pub(crate) trait DAGImplementor<NodeId, NodeData, EdgeData>: seal::Sealed {
    fn new(dummy_node_data: fn(&NodeData) -> NodeData) -> Self;

    fn peel_front(&mut self) -> impl Iterator<Item = NodeData>;

    fn is_empty(&self) -> bool;

    fn post_compose(&mut self, other: Self, edge_weights: fn() -> EdgeData);

    #[allow(dead_code)]
    fn coproduct(&mut self, other: Self);

    #[allow(dead_code)]
    fn all_parallel(
        dummy_node_data: fn(&NodeData) -> NodeData,
        more_nodes: impl IntoIterator<Item = NodeData>,
    ) -> Self;

    #[allow(dead_code)]
    fn all_sequantial(
        dummy_node_data: fn(&NodeData) -> NodeData,
        more_nodes: impl IntoIterator<Item = NodeData>,
    ) -> Self;
}

impl<NodeData, EdgeData> DAGImplementor<MyDAGNode, NodeData, EdgeData>
    for MyDAG<NodeData, EdgeData>
{
    fn new(dummy_node_data: fn(&NodeData) -> NodeData) -> Self {
        Self {
            underlying: StableDiGraph::with_capacity(0, 0),
            dummy_node_data,
        }
    }

    fn peel_front(&mut self) -> impl Iterator<Item = NodeData> {
        let mut front = Vec::with_capacity(self.underlying.node_count() >> 3);
        let idces = self.underlying.node_indices().collect::<Vec<_>>();
        for idx in idces {
            let has_no_incoming = self
                .underlying
                .neighbors_directed(idx, Direction::Incoming)
                .count()
                == 0;
            if has_no_incoming {
                if let Some(removed) = self.underlying.remove_node(idx) {
                    front.push(removed);
                }
            }
        }
        front.into_iter()
    }

    fn is_empty(&self) -> bool {
        self.underlying.node_count() == 0
    }

    fn post_compose(&mut self, other: Self, edge_weights: fn() -> EdgeData) {
        let source_other = other
            .underlying
            .externals(Direction::Incoming)
            .collect::<Vec<_>>();
        let target_self = self
            .underlying
            .externals(Direction::Outgoing)
            .collect::<Vec<_>>();
        let other_idx_to_self_idx = self.coproduct_helper(other);
        for cur_source_other in source_other {
            let as_in_self_now = *other_idx_to_self_idx
                .get(&cur_source_other)
                .expect("all nodes of other are in matched_indices");
            for cur_target_self in &target_self {
                self.underlying
                    .add_edge(*cur_target_self, as_in_self_now, edge_weights());
            }
        }
    }

    fn coproduct(&mut self, other: Self) {
        let _ = self.coproduct_helper(other);
    }

    fn all_parallel(
        dummy_node_data: fn(&NodeData) -> NodeData,
        more_nodes: impl IntoIterator<Item = NodeData>,
    ) -> Self {
        let mut to_return = Self::new(dummy_node_data);
        for new_node in more_nodes {
            to_return.underlying.add_node(new_node);
        }
        to_return
    }

    fn all_sequantial(
        _dummy_node_data: fn(&NodeData) -> NodeData,
        _more_nodes: impl IntoIterator<Item = NodeData>,
    ) -> Self {
        todo!("all sequential")
    }
}

impl<NodeData, EdgeData> MyDAG<NodeData, EdgeData> {
    fn coproduct_helper(&mut self, mut other: Self) -> HashMap<NodeIndex, NodeIndex> {
        let mut matched_indices = HashMap::with_capacity(other.underlying.node_count());
        let other_idcs = other.underlying.node_indices().collect::<Vec<_>>();
        for other_idx in other_idcs {
            let cur_weight = other
                .underlying
                .node_weight_mut(other_idx)
                .expect("in other graph by assumption");
            let mut dummy = (self.dummy_node_data)(cur_weight);
            core::mem::swap(cur_weight, &mut dummy);
            let new_idx = self.underlying.add_node(dummy);
            matched_indices.insert(other_idx, new_idx);
        }
        let other_edge_idces = other.underlying.edge_indices().collect::<Vec<_>>();
        for edge_idx in other_edge_idces {
            let (cur_source, cur_target) = other
                .underlying
                .edge_endpoints(edge_idx)
                .expect("is an edge in other graph by assumption");
            let cur_weight = other
                .underlying
                .remove_edge(edge_idx)
                .expect("is an edge in other graph by assumption");
            let cur_source = matched_indices
                .get(&cur_source)
                .expect("all nodes of other are in matched_indices");
            let cur_target = matched_indices
                .get(&cur_target)
                .expect("all nodes of other are in matched_indices");
            self.underlying
                .add_edge(*cur_source, *cur_target, cur_weight);
        }
        matched_indices
    }
}

impl<NodeData, EdgeData> TryFrom<StableDiGraph<NodeData, EdgeData>> for MyDAG<NodeData, EdgeData>
where
    NodeData: Default,
{
    type Error = Cycle<MyDAGNode>;

    fn try_from(underlying: StableDiGraph<NodeData, EdgeData>) -> Result<Self, Self::Error> {
        let _z = petgraph::algo::toposort(&underlying, None)?;
        Ok(Self {
            underlying,
            dummy_node_data: |_| NodeData::default(),
        })
    }
}
