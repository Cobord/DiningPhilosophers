An implementation of Dining Philosophers problem with Chandy/Misra method of having clean/dirty resources that can be requested and received by each Philosopher.

There is a `PhilosopherSystem` struct to do initialization with
- the bipartite graph of which philosopher needs which resources
- what functions each `Philosopher` needs to do when given a `Context` and access to those resources

After that we can feed in `Context` `PhilosopherIdentifier` pairs so that each `Philosopher` has a queue of what they want to do as soon as they have the resources to do so while also being fair and giving up those resources another `Philosopher` requests it.

These can also be fed in layer by layer if there is some DAG structure to all these tasks. This way we can make sure everything from one layer is done before the next batch that depends on that previous batch.
