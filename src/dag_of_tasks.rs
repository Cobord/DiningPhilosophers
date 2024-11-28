use std::hash::Hash;

use crate::{
    bipartite_graph::BipartiteGraph,
    communication_setup::{PhilosopherSystem, PhilosopherSystemError},
    dag_utils::{DAGImplementor, OnlyDAG},
    philosophers::PhilosopherJob,
};

#[derive(Debug)]
pub enum DAGPhilosopherSystemError<PhilosopherIdentifier> {
    UnderlyingPhilosopherSystemError(PhilosopherSystemError<PhilosopherIdentifier>),
}

#[allow(dead_code)]
pub struct DAGPhilosopherSystem<ResourceIdentifier, Resources, Context, PhilosopherIdentifier>
where
    ResourceIdentifier: Copy + Eq + Ord + Hash,
    PhilosopherIdentifier: Clone + Eq + Hash,
{
    philo_system: PhilosopherSystem<ResourceIdentifier, Resources, Context, PhilosopherIdentifier>,
    my_dag: OnlyDAG<(PhilosopherIdentifier, Context)>,
}

impl<ResourceIdentifier, Resources, Context, PhilosopherIdentifier>
    DAGPhilosopherSystem<ResourceIdentifier, Resources, Context, PhilosopherIdentifier>
where
    ResourceIdentifier: Copy + Eq + Ord + Hash,
    PhilosopherIdentifier: Clone + Eq + Ord + Hash,
    Resources: 'static,
    Context: 'static,
{
    #[allow(dead_code)]
    #[allow(clippy::type_complexity)]
    pub fn new(
        philo_rsc_graph: BipartiteGraph<PhilosopherIdentifier, ResourceIdentifier>,
        philo_jobs: Vec<PhilosopherJob<Context, Resources>>,
        starting_resources: Vec<(ResourceIdentifier, Resources)>,
    ) -> Result<Self, DAGPhilosopherSystemError<PhilosopherIdentifier>> {
        let new_underlying =
            PhilosopherSystem::new(philo_rsc_graph, philo_jobs, starting_resources)
                .map_err(DAGPhilosopherSystemError::UnderlyingPhilosopherSystemError)?;
        Ok(Self {
            philo_system: new_underlying,
            my_dag: OnlyDAG::new(),
        })
    }

    #[allow(dead_code)]
    fn validate(&self) -> bool {
        self.philo_system.validate()
    }

    #[allow(dead_code)]
    pub fn chain_more_tasks(&mut self, more_tasks: OnlyDAG<(PhilosopherIdentifier, Context)>) {
        self.my_dag.post_compose(more_tasks);
    }

    #[allow(dead_code)]
    pub fn run_system_fairly(&mut self, max_tries_per_layer: usize) -> bool
    where
        ResourceIdentifier: Send + 'static,
        PhilosopherIdentifier: Send + 'static,
        Context: Clone + Send + 'static,
        Resources: Send + 'static,
        PhilosopherIdentifier: core::fmt::Debug + core::fmt::Display,
        ResourceIdentifier: core::fmt::Debug,
        Resources: core::fmt::Debug,
    {
        while !self.my_dag.is_empty() {
            let next_layer = self.my_dag.peel_front();
            let mut tries_this_layer = 1;
            let mut all_finished = self.philo_system.run_system_fairly(next_layer);
            while !all_finished && tries_this_layer < max_tries_per_layer {
                all_finished = self.philo_system.clear_backlog();
                tries_this_layer += 1;
            }
            if !all_finished {
                return false;
            }
        }
        true
    }
}

mod test {

    #[test]
    fn five_philosophers() {
        use super::{DAGPhilosopherSystem, PhilosopherJob};
        use crate::bipartite_graph::BipartiteGraph;
        use nonempty::NonEmpty;

        const PHILOSOPHER_NAMES: [&str; 5] = [
            "Baruch Spinoza",
            "Gilles Deleuze",
            "Karl Marx",
            "Friedrich Nietzsche",
            "Michel Foucault",
        ];
        const NUM_PHILOSOPHERS: usize = PHILOSOPHER_NAMES.len();
        let same_job: PhilosopherJob<&str, u16> = |cur_philosopher, mut resources: NonEmpty<_>| {
            println!("{cur_philosopher} is eating");
            println!("They used {:?}", [resources[0], resources[1]]);
            resources[0] *= 2;
            resources[1] *= 2;
            resources
        };
        #[allow(clippy::type_complexity)]
        let all_jobs: Vec<PhilosopherJob<&str, u16>> = vec![same_job; NUM_PHILOSOPHERS];
        let mut philo_rsc_graph: BipartiteGraph<&str, usize> = BipartiteGraph::new();
        for philo in PHILOSOPHER_NAMES {
            philo_rsc_graph.add_a(philo);
        }
        for fork_num in 0..5 {
            philo_rsc_graph.add_b(fork_num);
        }
        for (fork_number, philo) in PHILOSOPHER_NAMES.iter().enumerate() {
            let next_fork = (fork_number + 1) % NUM_PHILOSOPHERS;
            philo_rsc_graph.add_edge(philo, next_fork);
            philo_rsc_graph.add_edge(philo, fork_number);
        }

        let mut phil_system = DAGPhilosopherSystem::new(
            philo_rsc_graph,
            all_jobs,
            [3, 5, 7, 11, 13].into_iter().enumerate().collect(),
        )
        .expect("system construction succeeds");

        assert!(phil_system.validate());
        let all_finished = phil_system.run_system_fairly(5);
        // no tasks have been put on so nothing to finish anyway
        assert!(all_finished);
    }
}
