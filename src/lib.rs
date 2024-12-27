pub mod bipartite_graph;
pub mod communication_setup;
pub mod dag_of_tasks;
pub mod dag_utils;
mod philosophers;
mod util;

pub use bipartite_graph::BipartiteGraph;
pub use communication_setup::{PhilosopherSystem, PhilosopherSystemError};
pub use dag_of_tasks::{DAGPhilosopherSystem, DAGPhilosopherSystemError};
pub use dag_utils::{MyDAG, MyDAGNode};
pub use philosophers::{CleanAndAnnotated, Philosopher};
