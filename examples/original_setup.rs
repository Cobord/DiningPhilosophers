use log::info;
use nonempty::NonEmpty;

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

    env_logger::init();
    log::set_max_level(log::LevelFilter::Trace);
    info!("Here");

    let _philo_jobs = [move |cur_philosopher: String, mut resources: NonEmpty<Resource>| {
        println!("{cur_philosopher} is eating");
        println!("They used {:?}", [resources[0], resources[1]]);
        resources[0] *= 2;
        resources[1] *= 2;
        resources
    }; NUM_PHILOSOPHERS]
        .into_iter()
        .collect::<Vec<_>>();
}
