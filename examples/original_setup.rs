use dining_philosophers::{BipartiteGraph, PhilosopherSystem};
use log::info;
use nonempty::NonEmpty;

type Resource = u16;

#[allow(clippy::needless_pass_by_value)]
fn philo_job(cur_philosopher: String, mut resources: NonEmpty<Resource>) -> NonEmpty<Resource> {
    println!("{cur_philosopher} is eating");
    println!("They used {:?}", [resources[0], resources[1]]);
    resources[0] *= 2;
    resources[1] *= 2;
    resources
}

fn main() {
    const PHILOSOPHER_NAMES: [&str; 5] = [
        "Baruch Spinoza",
        "Gilles Deleuze",
        "Karl Marx",
        "Friedrich Nietzsche",
        "Michel Foucault",
    ];
    const NUM_PHILOSOPHERS: usize = PHILOSOPHER_NAMES.len();

    env_logger::init();
    log::set_max_level(log::LevelFilter::Trace);
    info!("Here");

    let mut circle_bipartite_graph = BipartiteGraph::new();
    for philo_name in PHILOSOPHER_NAMES {
        circle_bipartite_graph.add_a(philo_name.to_string());
    }
    for fork_name in 0..5 {
        circle_bipartite_graph.add_b(fork_name);
    }
    #[allow(clippy::needless_range_loop)]
    for idx in 0..5 {
        let current_fork = idx;
        let next_fork = (idx + 1) % NUM_PHILOSOPHERS;
        circle_bipartite_graph.add_edge(PHILOSOPHER_NAMES[idx].to_string(), current_fork);
        circle_bipartite_graph.add_edge(PHILOSOPHER_NAMES[idx].to_string(), next_fork);
    }
    let fork_values = [3, 5, 7, 11, 13];
    let mut philo_system = PhilosopherSystem::new(
        circle_bipartite_graph,
        vec![philo_job, philo_job, philo_job, philo_job, philo_job],
        (0..NUM_PHILOSOPHERS).map(|z| (z, fork_values[z])).collect(),
    )
    .expect("System created correctly");
    let all_finished = philo_system.run_system_fairly(
        [
            (
                PHILOSOPHER_NAMES[0].to_string(),
                PHILOSOPHER_NAMES[0].to_string(),
            ),
            (
                PHILOSOPHER_NAMES[1].to_string(),
                PHILOSOPHER_NAMES[1].to_string(),
            ),
            (
                PHILOSOPHER_NAMES[4].to_string(),
                PHILOSOPHER_NAMES[4].to_string(),
            ),
            (
                PHILOSOPHER_NAMES[0].to_string(),
                PHILOSOPHER_NAMES[0].to_string(),
            ),
        ]
        .into_iter(),
    );
    println!("All finished : {all_finished}");
    println!("Total backlog length : {}", philo_system.how_many_backlog());
    let all_finished = philo_system.clear_backlog();
    println!("All finished after a call to clear_backlog : {all_finished}");
    println!("Total backlog length : {}", philo_system.how_many_backlog());
}
