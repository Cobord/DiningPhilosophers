use std::{
    collections::{HashMap, VecDeque},
    hash::Hash,
    sync::{
        mpsc::{self, Receiver, Sender},
        Arc, Mutex,
    },
    thread,
    time::Duration,
};

use nonempty::NonEmpty;

use crate::util::nonempty_sort;

#[allow(dead_code)]
pub struct CleanAndAnnotated<ResourceIdentifier, Resources, PhilosopherIdentifier>
where
    ResourceIdentifier: Copy + Eq + Ord + Hash,
    PhilosopherIdentifier: Clone + Eq + Hash,
{
    is_clean: bool,
    last_user: Option<PhilosopherIdentifier>,
    identifier: ResourceIdentifier,
    underlying: Resources,
}

impl<ResourceIdentifier, Resources, PhilosopherIdentifier>
    CleanAndAnnotated<ResourceIdentifier, Resources, PhilosopherIdentifier>
where
    ResourceIdentifier: Copy + Eq + Ord + Hash,
    PhilosopherIdentifier: Clone + Eq + Hash,
{
    pub fn new(underlying: Resources, identifier: ResourceIdentifier) -> Self {
        Self {
            is_clean: false,
            last_user: None,
            identifier,
            underlying,
        }
    }
}

impl<ResourceIdentifier, Resources, PhilosopherIdentifier> core::fmt::Debug
    for CleanAndAnnotated<ResourceIdentifier, Resources, PhilosopherIdentifier>
where
    ResourceIdentifier: Copy + Eq + Ord + Hash + core::fmt::Debug,
    PhilosopherIdentifier: Clone + Eq + Hash + core::fmt::Debug,
    Resources: core::fmt::Debug,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CleanAndAnnotated")
            .field("is_clean", &self.is_clean)
            .field("last_user", &self.last_user)
            .field("identifier", &self.identifier)
            .field("underlying", &self.underlying)
            .finish()
    }
}

#[allow(dead_code)]
pub struct Philosopher<ResourceIdentifier, Resources, Context, PhilosopherIdentifier>
where
    ResourceIdentifier: Copy + Eq + Ord + Hash,
    PhilosopherIdentifier: Clone + Eq + Hash,
{
    pub(crate) my_id: PhilosopherIdentifier,
    pub(crate) holding:
        Vec<CleanAndAnnotated<ResourceIdentifier, Resources, PhilosopherIdentifier>>,
    job: fn(Context, NonEmpty<Resources>) -> NonEmpty<Resources>,
    resource_sending: HashMap<
        PhilosopherIdentifier,
        Sender<CleanAndAnnotated<ResourceIdentifier, Resources, PhilosopherIdentifier>>,
    >,
    resource_receiving:
        Receiver<CleanAndAnnotated<ResourceIdentifier, Resources, PhilosopherIdentifier>>,
    request_receiving: Receiver<(PhilosopherIdentifier, ResourceIdentifier)>,
    request_sending:
        HashMap<ResourceIdentifier, Vec<Sender<(PhilosopherIdentifier, ResourceIdentifier)>>>,
    resources_needed: NonEmpty<ResourceIdentifier>,
    context_queue: VecDeque<Context>,
    job_count: Arc<Mutex<usize>>,
}

impl<ResourceIdentifier, Resources, Context, PhilosopherIdentifier>
    Philosopher<ResourceIdentifier, Resources, Context, PhilosopherIdentifier>
where
    ResourceIdentifier: Copy + Eq + Ord + Hash,
    PhilosopherIdentifier: Clone + Eq + Hash,
{
    pub fn new(
        my_id: PhilosopherIdentifier,
        mut starting_resources: Vec<
            CleanAndAnnotated<ResourceIdentifier, Resources, PhilosopherIdentifier>,
        >,
        job: fn(Context, NonEmpty<Resources>) -> NonEmpty<Resources>,
        mut resources_needed: NonEmpty<ResourceIdentifier>,
        resource_receiving: Receiver<
            CleanAndAnnotated<ResourceIdentifier, Resources, PhilosopherIdentifier>,
        >,
        request_receiving: Receiver<(PhilosopherIdentifier, ResourceIdentifier)>,
        job_count: Arc<Mutex<usize>>,
    ) -> Self {
        starting_resources.sort_by(|z1, z2| z1.identifier.cmp(&z2.identifier));
        nonempty_sort(&mut resources_needed);
        Self {
            my_id,
            holding: starting_resources,
            job,
            resource_sending: HashMap::new(),
            resource_receiving,
            request_receiving,
            request_sending: HashMap::new(),
            resources_needed,
            context_queue: VecDeque::new(),
            job_count,
        }
    }

    fn dummy_job(&mut self) -> bool {
        self.holding
            .sort_by(|z1, z2| z1.identifier.cmp(&z2.identifier));
        let have_all_needed = self.holding.len() == self.resource_sending.len()
            && self
                .holding
                .iter()
                .zip(self.resources_needed.iter())
                .all(|(z, w)| z.identifier == *w);
        if have_all_needed {
            for cur_resource in &mut self.holding {
                cur_resource.is_clean = false;
                cur_resource.last_user = Some(self.my_id.clone());
            }
        }
        have_all_needed
    }

    fn do_job(&mut self, ctx: Context) -> bool {
        self.holding
            .sort_by(|z1, z2| z1.identifier.cmp(&z2.identifier));
        let have_all_needed = self.holding.len() == self.resource_sending.len()
            && self
                .holding
                .iter()
                .zip(self.resources_needed.iter())
                .all(|(z, w)| z.identifier == *w);
        if have_all_needed {
            let num_holding = self.holding.len();
            let resources = self
                .holding
                .drain(0..num_holding)
                .map(|z| z.underlying)
                .collect::<Vec<_>>();
            let resources = NonEmpty::from_vec(resources).expect("already checked nonempty");
            let mut j = self.job_count.lock().expect("lock fine");
            *j -= 1;
            drop(j);
            let recovered = (self.job)(ctx, resources);
            assert_eq!(recovered.len(), self.resources_needed.len());
            for (recoved_resource, resource_id) in
                recovered.into_iter().zip(self.resources_needed.iter())
            {
                let mut annotated_resource = CleanAndAnnotated::new(recoved_resource, *resource_id);
                annotated_resource.is_clean = false;
                annotated_resource.last_user = Some(self.my_id.clone());
                self.holding.push(annotated_resource);
            }
        }
        have_all_needed
    }

    fn process_request(&mut self, timeout: Duration) -> [bool; 3] {
        let mut got_request = false;
        let mut had_the_resource = false;
        let mut gave_up_the_resource = false;
        if let Ok((who_wants_it, what_do_they_want)) = self.request_receiving.recv_timeout(timeout)
        {
            got_request = true;
            let idx_in_holding = self.holding.iter().enumerate().find_map(|(idx, z)| {
                if z.identifier == what_do_they_want && !z.is_clean {
                    Some(idx)
                } else {
                    None
                }
            });
            if let Some(idx_in_holding) = idx_in_holding {
                had_the_resource = true;
                if let Some(sender_to_them) = self.resource_sending.get(&who_wants_it) {
                    let mut to_send = self.holding.remove(idx_in_holding);
                    to_send.is_clean = true;
                    let send_status = sender_to_them.send(to_send);
                    gave_up_the_resource = true;
                    if let Err(mut z) = send_status {
                        z.0.is_clean = false;
                        self.holding.insert(idx_in_holding, z.0);
                        gave_up_the_resource = false;
                    }
                }
            }
        }
        [got_request, had_the_resource, gave_up_the_resource]
    }

    fn send_single_request(&mut self, which_one: Option<ResourceIdentifier>) -> bool {
        match which_one {
            None => {
                let held_rids = self
                    .holding
                    .iter()
                    .map(|z| z.identifier)
                    .collect::<Vec<_>>();
                let first_needed = self
                    .resources_needed
                    .iter()
                    .find(|rid| !held_rids.contains(*rid));
                if let Some(first_needed) = first_needed {
                    self.send_single_request(Some(*first_needed))
                } else {
                    false
                }
            }
            Some(which_one) => {
                if let Some(where_to_send_requests) = self.request_sending.get(&which_one) {
                    for cur_send_request in where_to_send_requests {
                        let _ = cur_send_request.send((self.my_id.clone(), which_one));
                    }
                    true
                } else {
                    false
                }
            }
        }
    }

    fn receive_resource(&mut self, timeout: Duration) -> bool {
        if let Ok(rcvd_resource) = self.resource_receiving.recv_timeout(timeout) {
            let where_to_insert = self
                .holding
                .binary_search_by(|probe| probe.identifier.cmp(&rcvd_resource.identifier));
            let where_to_insert = match where_to_insert {
                Ok(z) | Err(z) => z,
            };
            self.holding.insert(where_to_insert, rcvd_resource);
            return true;
        }
        false
    }

    pub fn peer_will_request(
        &mut self,
        who: PhilosopherIdentifier,
        sender: Sender<CleanAndAnnotated<ResourceIdentifier, Resources, PhilosopherIdentifier>>,
    ) {
        let _old_value = self.resource_sending.insert(who, sender);
    }

    pub fn i_will_request(
        &mut self,
        what: ResourceIdentifier,
        sender: Sender<(PhilosopherIdentifier, ResourceIdentifier)>,
    ) {
        if self.resources_needed.contains(&what) {
            let present = self.request_sending.entry(what).or_default();
            present.push(sender);
        }
    }

    pub fn get_resource_not_peer(
        &mut self,
        resource: CleanAndAnnotated<ResourceIdentifier, Resources, PhilosopherIdentifier>,
    ) -> Option<CleanAndAnnotated<ResourceIdentifier, Resources, PhilosopherIdentifier>> {
        if self.resources_needed.contains(&resource.identifier) {
            self.holding.push(resource);
            None
        } else {
            Some(resource)
        }
    }

    fn be_selfish(&mut self, ctx: Context, quick_timeout: Duration) -> bool
    where
        Context: Clone,
    {
        let mut j = self.job_count.lock().expect("lock fine");
        *j += 1;
        drop(j);
        while self.send_single_request(None) {
            self.receive_resource(quick_timeout);
        }
        while let Some(backlog_ctx) = self.context_queue.pop_front() {
            let did_job = self.do_job(backlog_ctx.clone());
            if !did_job {
                self.context_queue.push_front(backlog_ctx);
                self.context_queue.push_back(ctx);
                return false;
            }
        }
        let did_last_job = self.do_job(ctx.clone());
        if did_last_job {
            true
        } else {
            self.context_queue.push_back(ctx);
            false
        }
    }

    fn be_selfish_helper(&mut self, quick_timeout: Duration)
    where
        Context: Clone,
    {
        while self.send_single_request(None) {
            self.receive_resource(quick_timeout);
        }
        while let Some(backlog_ctx) = self.context_queue.pop_front() {
            let did_job = self.do_job(backlog_ctx.clone());
            if !did_job {
                self.context_queue.push_front(backlog_ctx);
                break;
            }
        }
    }

    #[allow(clippy::needless_pass_by_value)]
    fn be_selfless(&mut self, quick_timeout: Duration, stopper: Receiver<()>) {
        loop {
            self.process_request(quick_timeout);
            if let Ok(()) = stopper.recv_timeout(quick_timeout) {
                break;
            }
        }
    }

    #[allow(clippy::needless_pass_by_value)]
    fn be_selfless_helper(&mut self, quick_timeout: Duration) {
        loop {
            self.process_request(quick_timeout);
            let j = self.job_count.lock().expect("lock fine");
            if *j == 0 {
                break;
            }
            drop(j);
        }
    }

    // TODO:
    #[allow(dead_code, clippy::needless_pass_by_value)]
    fn just_clear_backlog(&mut self, quick_timeout: Duration, full_timeout: Duration)
    where
        Context: Clone,
        PhilosopherIdentifier: core::fmt::Display + core::fmt::Debug,
        ResourceIdentifier: core::fmt::Debug,
        Resources: core::fmt::Debug,
    {
        let mut est_time_used = Duration::from_millis(0);
        #[allow(unused_assignments)]
        while est_time_used < full_timeout {
            let j = self.job_count.lock().expect("lock fine");
            println!(
                "{} has {:?} and there are {} among everyone",
                self.my_id, self.holding, *j
            );
            if *j == 0 {
                drop(j);
                println!("{} has stopped", self.my_id);
                break;
            }
            if *j == self.context_queue.len() {
                drop(j);
                self.be_selfish_helper(quick_timeout);
                break;
            }
            drop(j);
            if self.context_queue.is_empty() {
                self.be_selfless_helper(quick_timeout);
                break;
            }
            let mut sent_a_request = self.send_single_request(None);
            let mut rcvd_resource = true;
            while rcvd_resource {
                rcvd_resource = self.receive_resource(quick_timeout);
                est_time_used += quick_timeout;
                if sent_a_request {
                    sent_a_request = self.send_single_request(None);
                }
                if rcvd_resource {
                    println!("got something {}", self.my_id);
                }
            }
            rcvd_resource = self.receive_resource(quick_timeout);
            est_time_used += quick_timeout;
            if self.context_queue.is_empty() {
                let _ = self.dummy_job();
            }
            while let Some(ctx) = self.context_queue.pop_front() {
                let did_job = self.do_job(ctx.clone());
                if did_job {
                    println!("one job down {}", self.my_id);
                } else {
                    self.context_queue.push_front(ctx);
                    break;
                }
            }
            println!("helping others clear their backlogs, {}", self.my_id);
            let mut process_more = self.holding.iter().any(|z| !z.is_clean);
            while process_more {
                let [req_rcv, _had_it, gave_it_up] = self.process_request(quick_timeout);
                est_time_used += quick_timeout;
                process_more = req_rcv && self.holding.iter().any(|z| !z.is_clean);
                if gave_it_up {
                    println!("sent something {}", self.my_id);
                }
            }
            let j = self.job_count.lock().expect("lock fine");
            if *j == 0 {
                println!("{} has stopped", self.my_id);
                break;
            }
            drop(j);
            println!(
                "either have more to do myself or more to offer {}",
                self.my_id
            );
        }
    }

    // TODO:
    #[allow(dead_code, clippy::needless_pass_by_value)]
    fn be_fair(
        &mut self,
        quick_timeout: Duration,
        full_timeout: Duration,
        contexts: Receiver<Option<Context>>,
    ) where
        Context: Clone,
        PhilosopherIdentifier: core::fmt::Debug + core::fmt::Display,
        ResourceIdentifier: core::fmt::Debug,
        Resources: core::fmt::Debug,
    {
        #[allow(unused_assignments)]
        loop {
            #[allow(clippy::if_not_else)]
            if !self.context_queue.is_empty() {
                let mut sent_a_request = self.send_single_request(None);
                let mut rcvd_resource = true;
                while rcvd_resource {
                    rcvd_resource = self.receive_resource(quick_timeout);
                    if sent_a_request {
                        sent_a_request = self.send_single_request(None);
                    }
                }
                rcvd_resource = self.receive_resource(quick_timeout);
            } else {
                let mut rcvd_resource = true;
                while rcvd_resource {
                    rcvd_resource = self.receive_resource(quick_timeout);
                }
            }
            let ctx_rcv = contexts.recv_timeout(quick_timeout);
            match ctx_rcv {
                Ok(Some(ctx)) => {
                    let mut j = self.job_count.lock().expect("lock fine");
                    *j += 1;
                    drop(j);
                    self.context_queue.push_back(ctx);
                    if let Some(ctx) = self.context_queue.pop_front() {
                        let did_job = self.do_job(ctx.clone());
                        if !did_job {
                            self.context_queue.push_front(ctx);
                        }
                    }
                }
                Ok(None) => {
                    // this is signalling that nobody is going to get any more jobs
                    // wait for all the others to get their last jobs
                    // and stabilize the `Arc<Mutex<usize>>` of `self.job_count`
                    // to it's maximum value
                    // that way in `just_clear_backlog` we can be sure that if it reaches 0
                    // it is safe to stop and we don't have to be selfless anymore
                    thread::sleep(quick_timeout);
                    self.just_clear_backlog(quick_timeout, full_timeout);
                    break;
                }
                Err(_) => {}
            }
            let mut process_more = self.holding.iter().any(|z| !z.is_clean);
            while process_more {
                let [req_rcv, _had_it, _gave_it_up] = self.process_request(quick_timeout);
                process_more = req_rcv && self.holding.iter().any(|z| !z.is_clean);
            }
        }
    }
}

pub(crate) fn make_one_selfish<ResourceIdentifier, Resources, Context, PhilosopherIdentifier>(
    which_selfish: usize,
    num_philosophers: usize,
    philosophers: Vec<Philosopher<ResourceIdentifier, Resources, Context, PhilosopherIdentifier>>,
    selfish_ctx: &Context,
) -> Vec<Philosopher<ResourceIdentifier, Resources, Context, PhilosopherIdentifier>>
where
    ResourceIdentifier: Copy + Eq + Ord + Hash + Send + 'static,
    PhilosopherIdentifier: Clone + Eq + Hash + Send + 'static,
    Context: Clone + Send + 'static,
    Resources: Send + 'static,
{
    let mut join_handles = Vec::with_capacity(num_philosophers);
    let mut stoppers = Vec::with_capacity(num_philosophers);
    for (idx, mut philo) in philosophers.into_iter().enumerate() {
        let selfish_ctx_clone = selfish_ctx.clone();
        let (stopper_send, stopper) = mpsc::channel();
        stoppers.push(stopper_send);
        let jh = if idx == which_selfish {
            thread::spawn(move || {
                philo.be_selfish(selfish_ctx_clone, Duration::from_millis(10));
                philo
            })
        } else {
            thread::spawn(move || {
                philo.be_selfless(Duration::from_millis(50), stopper);
                philo
            })
        };
        join_handles.push(jh);
    }
    let jh = join_handles.remove(which_selfish);
    let selfish_one = jh.join().expect("no problem joining");
    for stopper in stoppers {
        let _ = stopper.send(());
    }
    let mut to_return = join_handles
        .into_iter()
        .map(|jh| jh.join().expect("no problem joining"))
        .collect::<Vec<_>>();
    to_return.insert(which_selfish, selfish_one);
    to_return
}

// TODO:
/// the jobs that would be done with `given_contexts`
/// - if they have different philosophers, must commute
/// - if they use disjoint resources (which implies different philosophers) they must even interleave as well
/// - if they have the same philosopher, then they will execute in the order provided
///     both in the same threads
pub(crate) fn make_all_fair<ResourceIdentifier, Resources, Context, PhilosopherIdentifier>(
    num_philosophers: usize,
    philosophers: Vec<Philosopher<ResourceIdentifier, Resources, Context, PhilosopherIdentifier>>,
    given_contexts: impl Iterator<Item = (PhilosopherIdentifier, Context)>,
) -> Vec<Philosopher<ResourceIdentifier, Resources, Context, PhilosopherIdentifier>>
where
    ResourceIdentifier: Copy + Eq + Ord + Hash + Send + 'static,
    PhilosopherIdentifier: Clone + Eq + Hash + Send + 'static,
    Context: Clone + Send + 'static,
    Resources: Send + 'static,
    PhilosopherIdentifier: core::fmt::Debug + core::fmt::Display,
    ResourceIdentifier: core::fmt::Debug,
    Resources: core::fmt::Debug,
{
    let given_contexts = given_contexts
        .map(|(who_to_do, cur_ctx)| {
            let new_who_to_do = philosophers
                .iter()
                .enumerate()
                .find_map(|(z, w)| if w.my_id == who_to_do { Some(z) } else { None })
                .expect("each philosopher identifier is in the list");
            (new_who_to_do, cur_ctx)
        })
        .collect::<Vec<_>>();
    let mut join_handles = Vec::with_capacity(num_philosophers);
    let mut stoppers = Vec::with_capacity(num_philosophers);
    for mut philo in philosophers {
        let (stopper_send, stopper) = mpsc::channel();
        stoppers.push(stopper_send);
        let jh = thread::spawn(move || {
            philo.be_fair(
                Duration::from_millis(50),
                Duration::from_millis(300),
                stopper,
            );
            philo
        });
        join_handles.push(jh);
    }
    for (who_to_do, cur_ctx) in given_contexts {
        let _ = stoppers[who_to_do].send(Some(cur_ctx));
    }
    for stopper in stoppers {
        let _ = stopper.send(None);
    }
    join_handles
        .into_iter()
        .map(|jh| jh.join().expect("no problem joining"))
        .collect::<Vec<_>>()
}
