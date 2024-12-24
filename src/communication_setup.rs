use nonempty::NonEmpty;
use std::{
    collections::{HashMap, HashSet},
    hash::Hash,
    sync::{mpsc, Arc, Mutex},
};

use crate::{
    bipartite_graph::BipartiteGraph,
    philosophers::{CleanAndAnnotated, Philosopher, PhilosopherJob},
};

#[derive(Debug)]
pub enum PhilosopherSystemError<PhilosopherIdentifier> {
    PhilosopherWithNothingNeeded(PhilosopherIdentifier),
    #[allow(dead_code)]
    MismatchPhilosophersAndJob(usize, usize),
    NotAllNeededResourcesAtStart,
}

pub struct PhilosopherSystem<ResourceIdentifier, Resources, Context, PhilosopherIdentifier>
where
    ResourceIdentifier: Copy + Eq + Ord + Hash,
    PhilosopherIdentifier: Clone + Eq + Hash,
{
    philo_rsc_graph: BipartiteGraph<PhilosopherIdentifier, ResourceIdentifier>,
    philosophers: Vec<Philosopher<ResourceIdentifier, Resources, Context, PhilosopherIdentifier>>,
}

impl<ResourceIdentifier, Resources, Context, PhilosopherIdentifier>
    PhilosopherSystem<ResourceIdentifier, Resources, Context, PhilosopherIdentifier>
where
    ResourceIdentifier: Copy + Eq + Ord + Hash,
    PhilosopherIdentifier: Clone + Eq + Ord + Hash,
{
    /// create a new dining philosopher system where the resources needed by each philosopher
    /// is given via a `BipartiteGraph`
    /// each philosopher has an associated `PhilosopherJob`
    /// the jobs are given in order by `PhilosopherIdentifier`
    /// and there is `starting_resources` which are distributed amongst them so they can do
    /// those jobs (those are assumed to have distinct `ResourceIdentifier`s)
    /// # Errors
    /// - each philosopher must have a nonzero number of resources that are involved in it's job
    /// - there should be the same number of philosophers and jobs
    /// - all `Resources` that are needed by anybody must be in the provided in `starting_resources`
    /// # Panics
    /// the `HashMap` constructed has elements that are being accessed by construction
    /// so the `get` will succeed, but the method itself does not know that it is only being called
    /// with those values
    pub fn new(
        philo_rsc_graph: BipartiteGraph<PhilosopherIdentifier, ResourceIdentifier>,
        philo_jobs: Vec<PhilosopherJob<Context, Resources>>,
        starting_resources: Vec<(ResourceIdentifier, Resources)>,
    ) -> Result<Self, PhilosopherSystemError<PhilosopherIdentifier>> {
        let num_philos = philo_rsc_graph.num_a_nodes;
        if philo_jobs.len() != num_philos {
            return Err(PhilosopherSystemError::MismatchPhilosophersAndJob(
                num_philos,
                philo_jobs.len(),
            ));
        }
        let mut philosophers = Vec::with_capacity(num_philos);
        let mut resource_senders = Vec::with_capacity(num_philos);
        let mut request_senders = Vec::with_capacity(num_philos);
        let mut philo_2_idx: HashMap<PhilosopherIdentifier, usize> =
            HashMap::with_capacity(num_philos);
        let job_counter = Arc::new(Mutex::new(0));

        for (cur_philosopher, cur_job) in philo_rsc_graph
            .all_a_nodes_sorted()
            .into_iter()
            .zip(philo_jobs)
        {
            let (cur_resource_send, cur_resource_rcv) = mpsc::channel();
            let (cur_request_send, cur_request_rcv) = mpsc::channel();
            let resources_needed = NonEmpty::from_vec(philo_rsc_graph.neighbors_a(cur_philosopher))
                .ok_or_else(|| {
                    PhilosopherSystemError::PhilosopherWithNothingNeeded(cur_philosopher.clone())
                })?;
            let philosopher = Philosopher::new(
                cur_philosopher.clone(),
                vec![],
                cur_job,
                resources_needed,
                cur_resource_rcv,
                cur_request_rcv,
                job_counter.clone(),
            );
            philo_2_idx.insert(cur_philosopher.clone(), philosophers.len());
            philosophers.push(philosopher);
            resource_senders.push(cur_resource_send);
            request_senders.push(cur_request_send);
        }
        for cur_philosopher in &mut philosophers {
            #[allow(clippy::unnecessary_to_owned)]
            for cur_rsc_needed in cur_philosopher.view_needed().to_owned() {
                let who_might_have = philo_rsc_graph.neighbors_b(&cur_rsc_needed);
                let who_might_have_processed = who_might_have.into_iter().map(|pid| {
                    (
                        pid.clone(),
                        philo_2_idx.get(&pid).expect("in map").to_owned(),
                    )
                });
                for (who_might_have, idx_who_might_have) in who_might_have_processed {
                    cur_philosopher.i_will_request(
                        cur_rsc_needed,
                        request_senders[idx_who_might_have].clone(),
                    );
                    cur_philosopher.peer_will_request(
                        who_might_have,
                        resource_senders[idx_who_might_have].clone(),
                    );
                }
            }
        }
        let mut count_resources_assigned = 0;
        for (cur_rsc_id, cur_resource) in starting_resources {
            let who_wants_it = philo_rsc_graph.neighbors_b(&cur_rsc_id);
            if let Some(who_gets_it) = who_wants_it.into_iter().min() {
                let idx_who_gets_it = philo_2_idx.get(&who_gets_it).expect("in map").to_owned();
                let rcvd = philosophers[idx_who_gets_it]
                    .get_resource_not_peer(CleanAndAnnotated::new(cur_resource, cur_rsc_id))
                    .is_none();
                debug_assert!(rcvd, "they did not actually need it but they were a neighbor in the graph so they should have");
                count_resources_assigned += 1;
            }
        }
        let count_resources_needed_by_someone = philo_rsc_graph.num_nonisolated_b_nodes();
        if count_resources_assigned != count_resources_needed_by_someone {
            return Err(PhilosopherSystemError::NotAllNeededResourcesAtStart);
        }
        Ok(Self {
            philo_rsc_graph,
            philosophers,
        })
    }

    pub(crate) fn validate(&self) -> bool {
        let all_philo_ids: HashSet<PhilosopherIdentifier> = self
            .philosophers
            .iter()
            .map(|phil| phil.my_id.clone())
            .collect();
        if all_philo_ids != self.philo_rsc_graph.all_a_nodes {
            return false;
        }
        for philo in &self.philosophers {
            let mut count_needed = 0;
            for cur_needed in philo.view_needed() {
                count_needed += 1;
                if !self.philo_rsc_graph.contains_edge(&philo.my_id, cur_needed) {
                    return false;
                }
            }
            if self.philo_rsc_graph.count_neighbors_a(&philo.my_id) != count_needed {
                return false;
            }
        }
        // TODO: other expected validity constraints
        true
    }

    /// the jobs that would be done with `given_contexts`
    /// - if they have different philosophers, must commute (or at least the order doesn't matter at the very end)
    /// - if they use disjoint resources (which implies different philosophers) they must even interleave as well
    /// - if they have the same philosopher, then they will execute in the order provided
    ///     both in the same threads
    pub fn run_system_fairly(
        &mut self,
        given_contexts: impl Iterator<Item = (PhilosopherIdentifier, Context)>,
    ) -> bool
    where
        ResourceIdentifier: Send + 'static,
        PhilosopherIdentifier: Send + 'static,
        Context: Send + 'static,
        Resources: Send + 'static,
        PhilosopherIdentifier: core::fmt::Debug + core::fmt::Display,
        ResourceIdentifier: core::fmt::Debug,
        Resources: core::fmt::Debug,
    {
        let mut philos_before = vec![];
        core::mem::swap(&mut self.philosophers, &mut philos_before);
        let (philos_after, all_finished) = crate::philosophers::make_all_fair(
            self.philosophers.len(),
            philos_before,
            given_contexts,
        );
        self.philosophers = philos_after;
        all_finished
    }

    pub fn clear_backlog(&mut self) -> bool
    where
        ResourceIdentifier: Copy + Eq + Ord + Hash + Send + 'static,
        PhilosopherIdentifier: Clone + Eq + Hash + Send + 'static,
        Context: Send + 'static,
        PhilosopherIdentifier: core::fmt::Display + core::fmt::Debug,
        ResourceIdentifier: core::fmt::Debug,
        Resources: core::fmt::Debug + Send + 'static,
    {
        let mut philos_before = vec![];
        core::mem::swap(&mut self.philosophers, &mut philos_before);
        let (philos_after, all_finished) =
            crate::philosophers::just_clear_backlog(self.philosophers.len(), philos_before);
        self.philosophers = philos_after;
        all_finished
    }
}

mod test {

    #[test]
    fn five_philosophers() {
        use super::{BipartiteGraph, PhilosopherJob, PhilosopherSystem};
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

        let mut phil_system = PhilosopherSystem::new(
            philo_rsc_graph,
            all_jobs,
            [3, 5, 7, 11, 13].into_iter().enumerate().collect(),
        )
        .expect("system construction succeeds");

        assert!(phil_system.validate());

        println!("{:?}", phil_system.philosophers);

        let which_fairly = [2];
        let contexts_given = which_fairly
            .into_iter()
            .map(|which_now| (PHILOSOPHER_NAMES[which_now], PHILOSOPHER_NAMES[which_now]));
        phil_system.run_system_fairly(contexts_given);
    }
}
