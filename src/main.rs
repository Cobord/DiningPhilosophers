mod philosophers;
use std::sync::mpsc;

use philosophers::{make_all_fair, make_one_selfish, CleanAndAnnotated, Philosopher};
use rand::Rng;

type Resource = u16;

fn main() {
    const PHILOSOPHER_NAMES: [&str; 5] = [
        "Baruch Spinoza",
        "Gilles Deleuze",
        "Karl Marx",
        "Friedrich Nietzsche",
        "Michel Foucault",
    ];
    const NUM_PHILOSOPHERS: usize = PHILOSOPHER_NAMES.len();
    let mut philosophers = Vec::with_capacity(NUM_PHILOSOPHERS);
    let mut resource_senders = Vec::with_capacity(NUM_PHILOSOPHERS);
    let mut request_senders = Vec::with_capacity(NUM_PHILOSOPHERS);

    for (philo_id, cur_philosopher) in PHILOSOPHER_NAMES.iter().enumerate() {
        let cur_philosopher = (*cur_philosopher).to_string();
        let (cur_resource_send, cur_resource_rcv) = mpsc::channel();
        let (cur_request_send, cur_request_rcv) = mpsc::channel();
        let job = move |cur_philosopher, mut resources: Vec<Resource>| {
            println!("{cur_philosopher:?} is eating");
            println!("They used {resources:?}");
            resources[0] *= resources[0];
            resources[1] *= resources[1];
            resources
        };
        let cur_fork = philo_id;
        let next_fork = (philo_id + 1) % NUM_PHILOSOPHERS;
        let philosopher = Philosopher::new(
            cur_philosopher,
            vec![],
            job,
            vec![cur_fork, next_fork],
            cur_resource_rcv,
            cur_request_rcv,
        );
        philosophers.push(philosopher);
        resource_senders.push(cur_resource_send);
        request_senders.push(cur_request_send);
    }
    #[allow(clippy::needless_range_loop)]
    for fork_number in 0..NUM_PHILOSOPHERS {
        let next_fork = (fork_number + 1) % NUM_PHILOSOPHERS;
        let prev_fork = (fork_number + NUM_PHILOSOPHERS - 1) % NUM_PHILOSOPHERS;

        philosophers[fork_number].i_will_request(fork_number, request_senders[prev_fork].clone());
        philosophers[fork_number].i_will_request(next_fork, request_senders[next_fork].clone());

        let previous_philo = PHILOSOPHER_NAMES[prev_fork].to_string();
        let next_philo = PHILOSOPHER_NAMES[next_fork].to_string();
        philosophers[fork_number]
            .peer_will_request(previous_philo, resource_senders[prev_fork].clone());
        philosophers[fork_number]
            .peer_will_request(next_philo, resource_senders[next_fork].clone());
    }

    let fork_values = [2, 3, 5, 7, 11];
    for (idx, (philo, a_fork)) in philosophers.iter_mut().zip(fork_values).enumerate() {
        philo.get_resource_not_peer(CleanAndAnnotated::new(a_fork, idx));
    }

    let which_selfish = rand::thread_rng().gen_range(0..NUM_PHILOSOPHERS);
    let mut expected_fork_num0 = fork_values[which_selfish];
    let mut expected_fork_num1 = fork_values[(which_selfish + 1) % NUM_PHILOSOPHERS];
    if which_selfish == NUM_PHILOSOPHERS - 1 {
        std::mem::swap(&mut expected_fork_num0, &mut expected_fork_num1);
    }
    println!("Expected");
    println!("{:?} is eating", PHILOSOPHER_NAMES[which_selfish]);
    println!(
        "They used {:?}",
        vec![expected_fork_num0, expected_fork_num1]
    );

    let philosophers = make_one_selfish(
        which_selfish,
        NUM_PHILOSOPHERS,
        philosophers,
        &PHILOSOPHER_NAMES[which_selfish].to_string(),
    );
    for philo in &philosophers {
        println!("{:?} {:?}", philo.my_id, philo.holding);
    }

    let which_first = 0;
    let _philosophers = make_all_fair(
        NUM_PHILOSOPHERS,
        philosophers,
        [(which_first, PHILOSOPHER_NAMES[which_first].to_string())].into_iter(),
    );
}
