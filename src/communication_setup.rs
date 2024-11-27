use nonempty::NonEmpty;
use std::{
    collections::{HashMap, HashSet},
    hash::Hash,
    sync::{mpsc, Arc, Mutex},
};

use crate::philosophers::{CleanAndAnnotated, Philosopher};

#[allow(dead_code)]
pub struct BipartiteGraph<A, B> {
    num_a_nodes: usize,
    all_a_nodes: HashSet<A>,
    num_b_nodes: usize,
    all_b_nodes: HashSet<B>,
    edges: HashSet<(A, B)>,
}

impl<A, B> BipartiteGraph<A, B>
where
    A: Clone + Eq + Hash,
    B: Clone + Eq + Hash,
{
    #[allow(dead_code)]
    pub fn new() -> Self {
        Self {
            num_a_nodes: 0,
            all_a_nodes: HashSet::new(),
            num_b_nodes: 0,
            all_b_nodes: HashSet::new(),
            edges: HashSet::new(),
        }
    }

    pub fn add_a(&mut self, new_a: A) {
        let newly_inserted = self.all_a_nodes.insert(new_a);
        if newly_inserted {
            self.num_a_nodes += 1;
        }
    }

    pub fn add_b(&mut self, new_b: B) {
        let newly_inserted = self.all_b_nodes.insert(new_b);
        if newly_inserted {
            self.num_b_nodes += 1;
        }
    }

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

    pub fn contains_edge(&self, a: &A, b: &B) -> bool {
        self.edges.contains(&(a.clone(), b.clone()))
    }

    pub fn neighbors_a(&self, cur_node: &A) -> Vec<B> {
        self.edges
            .iter()
            .filter_map(|(a, b)| {
                if *a == *cur_node {
                    Some(b.clone())
                } else {
                    None
                }
            })
            .collect()
    }

    pub fn neighbors_b<'a>(&'a self, cur_node: &'a B) -> impl Iterator<Item = A> + 'a {
        self.edges.iter().filter_map(|(a, b)| {
            if *b == *cur_node {
                Some(a.clone())
            } else {
                None
            }
        })
    }
}

#[derive(Debug)]
pub enum PhilosopherSystemError<PhilosopherIdentifier> {
    PhilosopherWithNothingNeeded(PhilosopherIdentifier),
}

#[allow(dead_code)]
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
    #[allow(dead_code)]
    #[allow(clippy::type_complexity)]
    pub fn new(
        philo_rsc_graph: BipartiteGraph<PhilosopherIdentifier, ResourceIdentifier>,
        philo_jobs: Vec<fn(Context, NonEmpty<Resources>) -> NonEmpty<Resources>>,
        starting_resources: Vec<(ResourceIdentifier, Resources)>,
    ) -> Result<Self, PhilosopherSystemError<PhilosopherIdentifier>> {
        let num_philos = philo_rsc_graph.num_a_nodes;
        let mut philosophers = Vec::with_capacity(num_philos);
        let mut resource_senders = Vec::with_capacity(num_philos);
        let mut request_senders = Vec::with_capacity(num_philos);
        let mut philo_2_idx: HashMap<PhilosopherIdentifier, usize> =
            HashMap::with_capacity(num_philos);
        let job_counter = Arc::new(Mutex::new(0));

        for (cur_philosopher, cur_job) in philo_rsc_graph.all_a_nodes.iter().zip(philo_jobs) {
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
        for (cur_rsc_id, cur_resource) in starting_resources {
            let who_wants_it = philo_rsc_graph.neighbors_b(&cur_rsc_id);
            if let Some(who_gets_it) = who_wants_it.into_iter().min() {
                let idx_who_gets_it = philo_2_idx.get(&who_gets_it).expect("in map").to_owned();
                philosophers[idx_who_gets_it]
                    .get_resource_not_peer(CleanAndAnnotated::new(cur_resource, cur_rsc_id));
            }
        }
        Ok(Self {
            philo_rsc_graph,
            philosophers,
        })
    }

    #[allow(dead_code)]
    fn validate(&self) -> bool {
        let all_philo_ids: HashSet<PhilosopherIdentifier> = self
            .philosophers
            .iter()
            .map(|phil| phil.my_id.clone())
            .collect();
        if all_philo_ids != self.philo_rsc_graph.all_a_nodes {
            return false;
        }
        for philo in &self.philosophers {
            for cur_needed in philo.view_needed() {
                if !self.philo_rsc_graph.contains_edge(&philo.my_id, cur_needed) {
                    return false;
                }
            }
        }
        // TODO: ...
        true
    }

    #[allow(dead_code)]
    fn run_system_fairly(
        &mut self,
        contexts_given: impl Iterator<Item = (PhilosopherIdentifier, Context)>,
    ) where
        ResourceIdentifier: Send + 'static,
        PhilosopherIdentifier: Send + 'static,
        Context: Clone + Send + 'static,
        Resources: Send + 'static,
        PhilosopherIdentifier: core::fmt::Debug + core::fmt::Display,
        ResourceIdentifier: core::fmt::Debug,
        Resources: core::fmt::Debug,
    {
        let mut philos_before = vec![];
        core::mem::swap(&mut self.philosophers, &mut philos_before);
        let philos_after = crate::philosophers::make_all_fair(
            self.philosophers.len(),
            philos_before,
            contexts_given,
        );
        self.philosophers = philos_after;
    }
}

mod test {

    #[test]
    fn five_philosophers() {
        use super::{BipartiteGraph, PhilosopherSystem};
        use nonempty::NonEmpty;
        const PHILOSOPHER_NAMES: [&str; 5] = [
            "Baruch Spinoza",
            "Gilles Deleuze",
            "Karl Marx",
            "Friedrich Nietzsche",
            "Michel Foucault",
        ];
        const NUM_PHILOSOPHERS: usize = PHILOSOPHER_NAMES.len();
        let same_job: fn(&str, NonEmpty<u16>) -> NonEmpty<u16> =
            |cur_philosopher, mut resources: NonEmpty<_>| {
                println!("{cur_philosopher} is eating");
                println!("They used {:?}", [resources[0], resources[1]]);
                resources[0] *= 2;
                resources[1] *= 2;
                resources
            };
        #[allow(clippy::type_complexity)]
        let all_jobs: Vec<fn(&str, NonEmpty<u16>) -> NonEmpty<u16>> =
            vec![same_job; NUM_PHILOSOPHERS];
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
