use std::{collections::HashSet, hash::Hash};

/// a bipartite graph with A and B type nodes
/// we don't expect many nodes or edges
/// so it is not optimized
pub struct BipartiteGraph<A, B> {
    pub(crate) num_a_nodes: usize,
    pub(crate) all_a_nodes: HashSet<A>,
    pub(crate) num_b_nodes: usize,
    all_b_nodes: HashSet<B>,
    edges: HashSet<(A, B)>,
}

impl<A, B> Default for BipartiteGraph<A, B> {
    fn default() -> Self {
        Self {
            num_a_nodes: 0,
            all_a_nodes: HashSet::new(),
            num_b_nodes: 0,
            all_b_nodes: HashSet::new(),
            edges: HashSet::new(),
        }
    }
}

impl<A, B> BipartiteGraph<A, B>
where
    A: Clone + Eq + Hash,
    B: Clone + Eq + Hash,
{
    /// empty bipartite graph
    #[allow(dead_code)]
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// add an A type node
    pub fn add_a(&mut self, new_a: A) {
        let newly_inserted = self.all_a_nodes.insert(new_a);
        if newly_inserted {
            self.num_a_nodes += 1;
        }
    }

    /// add a B type node
    pub fn add_b(&mut self, new_b: B) {
        let newly_inserted = self.all_b_nodes.insert(new_b);
        if newly_inserted {
            self.num_b_nodes += 1;
        }
    }

    /// add an edge connecting an A type node and a B type node
    /// if either nodes are not in the graph already, they get added as well
    #[allow(dead_code)]
    pub fn add_edge(&mut self, from_a: A, to_b: B) {
        if !self.all_a_nodes.contains(&from_a) {
            self.add_a(from_a.clone());
        }
        if !self.all_b_nodes.contains(&to_b) {
            self.add_b(to_b.clone());
        }
        self.edges.insert((from_a, to_b));
    }

    /// is there an edge connecting this A type node to this B type node
    pub fn contains_edge(&self, a: &A, b: &B) -> bool {
        self.edges.contains(&(a.clone(), b.clone()))
    }

    /// give all the B type neighbors for this A type node
    pub fn neighbors_a<'a>(&'a self, cur_node: &'a A) -> impl Iterator<Item = B> + 'a {
        self.edges.iter().filter_map(|(a, b)| {
            if *a == *cur_node {
                Some(b.clone())
            } else {
                None
            }
        })
    }

    /// how many B type neighbors does this A type node have
    pub fn count_neighbors_a(&self, cur_node: &A) -> usize {
        self.edges.iter().filter(|(a, _b)| *a == *cur_node).count()
    }

    /// give all the A type neighbors for this B type node
    pub fn neighbors_b<'a>(&'a self, cur_node: &'a B) -> impl Iterator<Item = A> + 'a {
        self.edges.iter().filter_map(|(a, b)| {
            if *b == *cur_node {
                Some(a.clone())
            } else {
                None
            }
        })
    }

    /// how many A type neighbors does this B type node have
    #[allow(dead_code)]
    pub fn count_neighbors_b(&self, cur_node: &B) -> usize {
        self.edges.iter().filter(|(_a, b)| *b == *cur_node).count()
    }

    /// all the A type nodes sorted provided A implements `Ord`
    #[must_use]
    pub fn all_a_nodes_sorted(&self) -> Vec<&A>
    where
        A: Ord,
    {
        let mut all_a_vec = self.all_a_nodes.iter().collect::<Vec<_>>();
        all_a_vec.sort_unstable_by(|&a, &b| a.cmp(b));
        all_a_vec
    }

    /// all the B type nodes sorted provided B implements `Ord`
    #[allow(dead_code)]
    #[must_use]
    pub fn all_b_nodes_sorted(&self) -> Vec<&B>
    where
        B: Ord,
    {
        let mut all_b_vec = self.all_b_nodes.iter().collect::<Vec<_>>();
        all_b_vec.sort_unstable_by(|&a, &b| a.cmp(b));
        all_b_vec
    }

    /// how many B type nodes are there that have at least one edge
    #[must_use]
    pub fn num_nonisolated_b_nodes(&self) -> usize {
        self.all_b_nodes
            .iter()
            .filter(|&b| self.neighbors_b(b).next().is_some())
            .count()
    }

    /// how many A type nodes are there that have at least one edge
    #[allow(dead_code)]
    #[must_use]
    pub fn num_nonisolated_a_nodes(&self) -> usize {
        self.all_a_nodes
            .iter()
            .filter(|&a| self.neighbors_a(a).next().is_some())
            .count()
    }
}
