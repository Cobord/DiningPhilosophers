use std::{
    collections::{HashMap, VecDeque},
    hash::Hash,
    sync::{
        mpsc::{self, Receiver, RecvTimeoutError, Sender},
        Arc, Mutex,
    },
    thread::{sleep, spawn},
    time::{Duration, Instant},
};

use nonempty::NonEmpty;

use crate::util::nonempty_sort;

const QUICK_TIMEOUT: Duration = Duration::from_millis(20);
const FULL_TIMEOUT: Duration = Duration::from_millis(100);

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
    /// a `Resources` starts off as dirty and with no last user
    pub(crate) fn new(underlying: Resources, identifier: ResourceIdentifier) -> Self {
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

/*
pub type PhilosopherJob<Context, Resources> =
    Box<dyn Fn(Context, NonEmpty<Resources>) -> NonEmpty<Resources> + Send + 'static>;
*/
pub type PhilosopherJob<Context, Resources> =
    fn(Context, NonEmpty<Resources>) -> NonEmpty<Resources>;

#[allow(dead_code)]
pub(crate) fn make_many_same_job<Context, Resources>(
    f: fn(Context, NonEmpty<Resources>) -> NonEmpty<Resources>,
    how_many: usize,
) -> Vec<PhilosopherJob<Context, Resources>> {
    let mut to_return = Vec::with_capacity(how_many);
    for _ in 0..how_many {
        to_return.push(f);
    }
    to_return
}

#[allow(dead_code)]
pub(crate) fn make_one_job<Context, Resources>(
    f: fn(Context, NonEmpty<Resources>) -> NonEmpty<Resources>,
) -> PhilosopherJob<Context, Resources> {
    f
}

pub struct Philosopher<ResourceIdentifier, Resources, Context, PhilosopherIdentifier>
where
    ResourceIdentifier: Copy + Eq + Ord + Hash,
    PhilosopherIdentifier: Clone + Eq + Hash + core::fmt::Debug,
{
    pub(crate) my_id: PhilosopherIdentifier,
    pub(crate) holding:
        Vec<CleanAndAnnotated<ResourceIdentifier, Resources, PhilosopherIdentifier>>,
    job: PhilosopherJob<Context, Resources>,
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

#[allow(clippy::missing_fields_in_debug)]
impl<ResourceIdentifier, Resources, Context, PhilosopherIdentifier> core::fmt::Debug
    for Philosopher<ResourceIdentifier, Resources, Context, PhilosopherIdentifier>
where
    ResourceIdentifier: Copy + Eq + Ord + Hash + core::fmt::Debug,
    PhilosopherIdentifier: Clone + Eq + Hash + core::fmt::Debug,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Philosopher")
            .field("my_id", &self.my_id)
            .field(
                "holding",
                &self
                    .holding
                    .iter()
                    .map(|z| (z.identifier, z.is_clean))
                    .collect::<Vec<_>>(),
            )
            .field("resources_needed", &self.resources_needed)
            .field("context_queue length", &self.context_queue.len())
            .finish()
    }
}

impl<ResourceIdentifier, Resources, Context, PhilosopherIdentifier>
    Philosopher<ResourceIdentifier, Resources, Context, PhilosopherIdentifier>
where
    ResourceIdentifier: Copy + Eq + Ord + Hash,
    PhilosopherIdentifier: Clone + Eq + Hash + core::fmt::Debug,
{
    /// a `Philosopher` has
    /// - a `PhilosopherIdentifier`
    /// - some `starting_resources`
    /// - a `PhilosopherJob` to execute when it gets a `Context` and access to `resources_needed`
    /// - the `resources_needed` `ResourcesIdentifier`s to know what to request and receive
    /// - channels for those sending requests for resources, receiving resources, receiving requesting for resources and sending resources
    /// - a `job_count` which allows this to change behavior depending on the total number of jobs in the system that are not just on
    ///     this `Philosopher` (in it's `context_queue` backlog)
    pub(crate) fn new(
        my_id: PhilosopherIdentifier,
        mut starting_resources: Vec<
            CleanAndAnnotated<ResourceIdentifier, Resources, PhilosopherIdentifier>,
        >,
        job: PhilosopherJob<Context, Resources>,
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

    /// a view into `resources_needed`
    /// don't make public because never want mutable access to this
    pub fn view_needed(&self) -> &NonEmpty<ResourceIdentifier> {
        &self.resources_needed
    }

    /// have no backlog of contexts that still need to go through `self.job`
    pub fn has_no_backlog(&self) -> bool {
        self.context_queue.is_empty()
    }

    /// how many in backlog of contexts that still need to go through `self.job`
    pub fn count_backlog(&self) -> usize {
        self.context_queue.len()
    }

    /// make all resources `self` is holding dirty so they will be given up when requested
    #[allow(dead_code)]
    fn make_all_dirty(&mut self) {
        for cur_resource in &mut self.holding {
            cur_resource.is_clean = false;
            cur_resource.last_user = Some(self.my_id.clone());
        }
    }

    /// if we have all the `resources_needed` to do the `job`
    /// with them and the given `ctx` `Context`
    /// we get back the possibly mutated `Resources`
    /// put them back into `holding`, but now as dirty so we will give them
    /// up if requested rather than hogging them for only our jobs
    /// return None if we actually had everything needed to do the job (which also means we did the job)
    /// otherwise give the context back to put it into backlog
    fn do_job(&mut self, ctx: Context) -> Option<Context> {
        self.holding
            .sort_by(|z1, z2| z1.identifier.cmp(&z2.identifier));
        let have_all_needed = self.holding.len() == self.resources_needed.len()
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
            None
        } else {
            Some(ctx)
        }
    }

    /// listen for the requests from other `Philosopher`'s for resources
    /// send it to them if we have it and it is dirty
    /// they get it as a clean version, so they will keep it for their own use first
    /// before we would request it and try to steal it back
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

    /// send a request for a single resource that we need
    /// return
    /// - whether a request should have gone out
    /// - whether a request actually went out
    fn send_single_request(&mut self, which_one: Option<ResourceIdentifier>) -> [bool; 2] {
        match which_one {
            None => {
                // do for the first one we need but don't have
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
                    [false, false]
                }
            }
            Some(which_one) => {
                // if we don't actually need this resource it won't be in request_sending as a key
                // so no requests will be sent in this case
                // but if we do need it, we should have the channels to all the other `Philosopher`'s who
                // can give it to us
                if let Some(where_to_send_requests) = self.request_sending.get(&which_one) {
                    let mut a_request_flew_off = false;
                    for cur_send_request in where_to_send_requests {
                        let flew_off = cur_send_request.send((self.my_id.clone(), which_one));
                        a_request_flew_off |= flew_off.is_ok();
                    }
                    [true, a_request_flew_off]
                } else {
                    [false, false]
                }
            }
        }
    }

    /// attempt to receive a resource on the `resource_receiving` channel
    /// if it is received, put it `holding` while maintaining the order
    /// return if a resource was received or not
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

    /// the other `Philosopher` `who`
    /// might send a request for a `Resources` from this `self`
    /// so put the `sender` that can send it to them into `resource_sending`
    pub(crate) fn peer_will_request(
        &mut self,
        who: PhilosopherIdentifier,
        sender: Sender<CleanAndAnnotated<ResourceIdentifier, Resources, PhilosopherIdentifier>>,
    ) {
        let _old_value = self.resource_sending.insert(who, sender);
    }

    /// this `Philosopher` needs the resource associated with
    /// the `ResourceIdentifier` `what`
    /// so put it in `request_sending` so it can send the request
    /// to the other `Philosopher` which holds the corresponding `Receiver`
    pub(crate) fn i_will_request(
        &mut self,
        what: ResourceIdentifier,
        sender: Sender<(PhilosopherIdentifier, ResourceIdentifier)>,
    ) {
        if self.resources_needed.contains(&what) {
            let present = self.request_sending.entry(what).or_default();
            present.push(sender);
        }
    }

    pub(crate) fn get_resource_not_peer(
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

    /// attempt to grab all the resources I need
    /// then do all the tasks in my backlog
    /// if for some reason peers weren't giving it up
    /// return false to say we didn't manage to finish everything
    /// and puts `ctx` into the backlog too
    /// then do `ctx` as the last thing
    /// return true to say `ctx` was finished after all of the backlog
    fn be_selfish(&mut self, ctx: Context, quick_timeout: Duration) -> bool {
        let mut j = self.job_count.lock().expect("lock fine");
        *j += 1;
        drop(j);
        while self.send_single_request(None)[1] {
            let _did_receive_something = self.receive_resource(quick_timeout);
        }
        while let Some(backlog_ctx) = self.context_queue.pop_front() {
            let did_job = self.do_job(backlog_ctx);
            if let Some(backlog_ctx) = did_job {
                self.context_queue.push_front(backlog_ctx);
                self.context_queue.push_back(ctx);
                return false;
            }
        }
        let did_last_job = self.do_job(ctx);
        if let Some(ctx) = did_last_job {
            self.context_queue.push_back(ctx);
            false
        } else {
            true
        }
    }

    /// attempt to grab all the resources I need
    /// then do all the tasks in my backlog
    /// if there is any failure when trying to do a job
    /// because not all resources are obtained etc,
    /// then put the failed context back at the front of the backlog
    /// the endstate is either a completely empty backlog in the happy path
    /// or some backlog still left if some acquisition or job went wrong
    fn be_selfish_helper(&mut self, quick_timeout: Duration, full_timeout: Duration) {
        let mut est_time_used = Duration::from_secs(0);
        while self.send_single_request(None)[1] {
            let mut did_receive_something = self.receive_resource(quick_timeout);
            est_time_used += quick_timeout;
            while !did_receive_something {
                did_receive_something = self.receive_resource(quick_timeout);
                est_time_used += quick_timeout;
                if est_time_used > full_timeout {
                    break;
                }
            }
            if est_time_used > 10 * full_timeout {
                break;
            }
        }
        while let Some(backlog_ctx) = self.context_queue.pop_front() {
            let did_job = self.do_job(backlog_ctx);
            if let Some(backlog_ctx) = did_job {
                self.context_queue.push_front(backlog_ctx);
                break;
            }
        }
    }

    /// give up the resources am holding to anyone who requests them
    /// when receive the signal to stop or that transmitter has disconnected, then stop
    fn be_selfless(&mut self, quick_timeout: Duration, stopper: Receiver<()>) {
        loop {
            self.process_request(quick_timeout);
            match stopper.recv_timeout(quick_timeout) {
                Ok(()) | Err(RecvTimeoutError::Disconnected) => {
                    drop(stopper);
                    break;
                }
                Err(RecvTimeoutError::Timeout) => {}
            }
        }
    }

    /// give up the resources am holding to anyone who requests them
    /// when there are no more jobs left in the system as measured by the mutex
    /// then stop
    fn be_selfless_helper(&mut self, quick_timeout: Duration, full_timeout: Duration) {
        let mut est_time_used = Duration::from_secs(0);
        loop {
            self.process_request(quick_timeout);
            est_time_used += quick_timeout;
            let j = self.job_count.lock().expect("lock fine");
            if *j == 0 {
                break;
            }
            drop(j);
            if est_time_used > 20 * full_timeout {
                println!("{:?} ran out of time", self.my_id);
                break;
            }
        }
    }

    /// only clear backlog
    /// be selfish if we have the only jobs in the system
    /// be selfless if we have none of the backlog
    /// otherwise it is a combination of doing own jobs and responding to requests
    /// can stop when the number of jobs in the system reaches 0
    /// or with the `full_timeout`
    fn just_clear_backlog(&mut self, quick_timeout: Duration, full_timeout: Duration) {
        let mut est_time_used = Duration::from_millis(0);
        let start_time = Instant::now();
        while est_time_used < full_timeout {
            let elapsed = start_time.elapsed();
            if elapsed > 5 * full_timeout {
                break;
            }
            let j = self.job_count.lock().expect("lock fine");
            if *j > 0 && *j == self.context_queue.len() {
                //println!("{:?} is going to be selfish", self.my_id);
                drop(j);
                self.be_selfish_helper(quick_timeout, full_timeout);
                break;
            }
            drop(j);
            if self.context_queue.is_empty() {
                //println!("{:?} is going to be selfless", self.my_id);
                self.be_selfless_helper(quick_timeout, full_timeout);
                break;
            }
            let [mut had_request_to_send, mut sent_a_request] = self.send_single_request(None);
            let mut count_failed_send = u8::from(had_request_to_send && !sent_a_request);
            let mut rcvd_resource = true;
            while rcvd_resource {
                rcvd_resource = self.receive_resource(quick_timeout);
                est_time_used += quick_timeout;
                if had_request_to_send && count_failed_send < 3 {
                    [had_request_to_send, sent_a_request] = self.send_single_request(None);
                    if had_request_to_send && sent_a_request {
                        count_failed_send = 0;
                    } else if had_request_to_send {
                        count_failed_send += 1;
                    } else {
                        count_failed_send = 0;
                    }
                }
                if rcvd_resource {
                    // I got something, I'm going to try to receive more
                    if est_time_used > full_timeout {
                        break;
                    }
                }
            }
            _ = self.receive_resource(quick_timeout);
            est_time_used += quick_timeout;
            while let Some(ctx) = self.context_queue.pop_front() {
                let did_job = self.do_job(ctx);
                if let Some(ctx) = did_job {
                    self.context_queue.push_front(ctx);
                    break;
                }
            }
            let mut process_more = self.holding.iter().any(|z| !z.is_clean);
            while process_more {
                let [req_rcv, _had_it, _gave_it_up] = self.process_request(quick_timeout);
                est_time_used += quick_timeout;
                process_more = req_rcv && self.holding.iter().any(|z| !z.is_clean);
                if est_time_used > full_timeout {
                    break;
                }
            }
            let j = self.job_count.lock().expect("lock fine");
            if *j == 0 {
                break;
            }
            drop(j);
        }
    }

    /// a combination of selfishly doing own jobs and selflessly responding to requests
    /// can wind down when receive a stop signal, then we can move to `just_clear_backlog` stage
    /// new jobs received on `context_or_stop` go into our backlog
    fn be_fair(
        &mut self,
        quick_timeout: Duration,
        full_timeout: Duration,
        context_or_stop: Receiver<Option<Context>>,
    ) {
        loop {
            if self.context_queue.is_empty() {
                let mut rcvd_resource = true;
                while rcvd_resource {
                    rcvd_resource = self.receive_resource(quick_timeout);
                }
            } else {
                let [mut had_request_to_send, mut sent_a_request] = self.send_single_request(None);
                let mut count_failed_send = u8::from(had_request_to_send && !sent_a_request);
                let mut rcvd_resource = true;
                while rcvd_resource {
                    rcvd_resource = self.receive_resource(quick_timeout);
                    if had_request_to_send && count_failed_send < 3 {
                        [had_request_to_send, sent_a_request] = self.send_single_request(None);
                        if had_request_to_send && sent_a_request {
                            count_failed_send = 0;
                        } else if had_request_to_send {
                            count_failed_send += 1;
                        } else {
                            count_failed_send = 0;
                        }
                    }
                }
                _ = self.receive_resource(quick_timeout);
            }
            let ctx_rcv = context_or_stop.recv_timeout(quick_timeout);
            match ctx_rcv {
                Ok(Some(ctx)) => {
                    let mut j = self.job_count.lock().expect("lock fine");
                    *j += 1;
                    drop(j);
                    self.context_queue.push_back(ctx);
                    if let Some(ctx) = self.context_queue.pop_front() {
                        let did_job = self.do_job(ctx);
                        if let Some(ctx) = did_job {
                            self.context_queue.push_front(ctx);
                        }
                    }
                }
                Ok(None) => {
                    // this is signalling that nobody is going to get any more jobs
                    // wait for all the others to get their last jobs
                    // and stabilize the `Arc<Mutex<usize>>` of `self.job_count`
                    // to it's maximum value
                    // CAUTION: there is no guarantee that this amount of waiting is enough
                    //      This is an assumption
                    // that way in `just_clear_backlog` we can be sure that if it reaches 0
                    // it is safe to stop and we don't have to be selfless anymore
                    sleep(full_timeout);
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
        drop(context_or_stop);
    }
}

/// only 1 philosopher will be doing work associated with `selfish_ctx`
/// all the others will be listening to requests for resources from them
/// and providing them in return, doing no work or using those resources themselves
/// we don't know who has the resources for the work to be done among all of `philosophers`
/// so they do so with the request, response mechanism
#[allow(dead_code)]
pub(crate) fn make_one_selfish<ResourceIdentifier, Resources, Context, PhilosopherIdentifier>(
    which_selfish: usize,
    num_philosophers: usize,
    philosophers: Vec<Philosopher<ResourceIdentifier, Resources, Context, PhilosopherIdentifier>>,
    selfish_ctx: &Context,
) -> Vec<Philosopher<ResourceIdentifier, Resources, Context, PhilosopherIdentifier>>
where
    ResourceIdentifier: Copy + Eq + Ord + Hash + Send + 'static,
    PhilosopherIdentifier: Clone + Eq + Hash + core::fmt::Debug + Send + 'static,
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
            spawn(move || {
                philo.be_selfish(selfish_ctx_clone, QUICK_TIMEOUT / 5);
                philo
            })
        } else {
            spawn(move || {
                philo.be_selfless(QUICK_TIMEOUT, stopper);
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

/// the jobs that would be done with `given_contexts`
/// - if they have different philosophers, must commute (or at least the order doesn't matter at the very end)
/// - if they use disjoint resources (which implies different philosophers) they must even interleave as well
/// - if they have the same philosopher, then they will execute in the order provided
///     both in the same threads
pub(crate) fn make_all_fair<ResourceIdentifier, Resources, Context, PhilosopherIdentifier>(
    num_philosophers: usize,
    philosophers: Vec<Philosopher<ResourceIdentifier, Resources, Context, PhilosopherIdentifier>>,
    given_contexts: impl Iterator<Item = (PhilosopherIdentifier, Context)>,
) -> (
    Vec<Philosopher<ResourceIdentifier, Resources, Context, PhilosopherIdentifier>>,
    bool,
)
where
    ResourceIdentifier: Copy + Eq + Ord + Hash + Send + 'static,
    PhilosopherIdentifier: Clone + Eq + Hash + Send + 'static,
    Context: Send + 'static,
    Resources: Send + 'static,
    PhilosopherIdentifier: core::fmt::Debug + core::fmt::Display,
    ResourceIdentifier: core::fmt::Debug,
    Resources: core::fmt::Debug,
{
    let mut philo_2_idx: HashMap<_, _> = HashMap::with_capacity(num_philosophers);
    #[allow(unused_variables)]
    let mut how_many_contexts_already = 0;
    for (idx, philo) in philosophers.iter().enumerate() {
        philo_2_idx.insert(philo.my_id.clone(), idx);
        how_many_contexts_already += philo.context_queue.len();
    }
    let mut join_handles = Vec::with_capacity(num_philosophers);
    let mut work_or_stop_signals = Vec::with_capacity(num_philosophers);
    for mut philo in philosophers {
        let (work_or_stop_send, work_or_stop) = mpsc::channel();
        work_or_stop_signals.push(work_or_stop_send);
        let jh = spawn(move || {
            philo.be_fair(QUICK_TIMEOUT, FULL_TIMEOUT, work_or_stop);
            philo
        });
        join_handles.push(jh);
    }
    #[allow(unused_variables)]
    let mut how_many_sent_now = 0u8;
    #[allow(clippy::explicit_counter_loop)]
    for (who_to_do, cur_ctx) in given_contexts {
        how_many_sent_now += 1;
        let sent_work_2_philo = work_or_stop_signals[*philo_2_idx
            .get(&who_to_do)
            .expect("this philosopher exists")]
        .send(Some(cur_ctx));
        // CAUTION: what to do if sending work to a philosopher doesn't work
        // they are using `be_fair` so they should be listening on their `context_or_stop`
        // not ignoring it
        #[allow(clippy::items_after_statements)]
        const PANIC_IF_SEND_FAILS: bool = true;
        if PANIC_IF_SEND_FAILS {
            let () = sent_work_2_philo.expect("sending work to a philosopher suceeds");
        } else {
            // that work just doesn't get done
        }
    }
    sleep(FULL_TIMEOUT);
    for stopper in work_or_stop_signals {
        let _ = stopper.send(None);
    }
    let all_philos_now = join_handles
        .into_iter()
        .map(|jh| jh.join().expect("no problem joining"))
        .collect::<Vec<_>>();
    let all_finished = all_philos_now.iter().all(|p| p.context_queue.is_empty());
    (all_philos_now, all_finished)
}

/// all the philosophers do `just_clear_backlog`
pub(crate) fn just_clear_backlog<ResourceIdentifier, Resources, Context, PhilosopherIdentifier>(
    num_philosophers: usize,
    philosophers: Vec<Philosopher<ResourceIdentifier, Resources, Context, PhilosopherIdentifier>>,
) -> (
    Vec<Philosopher<ResourceIdentifier, Resources, Context, PhilosopherIdentifier>>,
    bool,
)
where
    ResourceIdentifier: Copy + Eq + Ord + Hash + Send + 'static,
    PhilosopherIdentifier: Clone + Eq + Hash + Send + 'static,
    Context: Send + 'static,
    PhilosopherIdentifier: core::fmt::Display + core::fmt::Debug,
    ResourceIdentifier: core::fmt::Debug,
    Resources: core::fmt::Debug + Send + 'static,
{
    let mut philo_2_idx: HashMap<_, _> = HashMap::with_capacity(num_philosophers);
    for (idx, philo) in philosophers.iter().enumerate() {
        philo_2_idx.insert(philo.my_id.clone(), idx);
    }
    let mut join_handles = Vec::with_capacity(num_philosophers);
    for mut philo in philosophers {
        let jh = spawn(move || {
            philo.just_clear_backlog(QUICK_TIMEOUT, FULL_TIMEOUT);
            philo
        });
        join_handles.push(jh);
    }
    sleep(FULL_TIMEOUT);
    let all_philos_now = join_handles
        .into_iter()
        .map(|jh| jh.join().expect("no problem joining"))
        .collect::<Vec<_>>();
    let all_finished = all_philos_now.iter().all(|p| p.context_queue.is_empty());
    (all_philos_now, all_finished)
}
