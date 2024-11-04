# maelstrom_study_notes
Repo for experimenting [Maelstrom](https://github.com/jepsen-io/maelstrom?tab=readme-ov-file), a workbench for learning distributed systems, and solution for the Distributed Systems challenge in Rust https://fly.io/dist-sys/

## Nodes and Networks

A Maelstrom test simulates a distributed system by running many *nodes*, and a
network which routes *messages* between them. Each node has a unique string
identifier, used to route messages to and from that node.

- Nodes `n1`, `n2`, `n3`, etc. are instances of the binary you pass to
  Maelstrom. These nodes implement whatever distributed algorithm you're trying
  to build: for instance, a key-value store. You can think of these as
  *servers*, in that they accept requests from clients and send back responses.

- Nodes `c1`, `c2`, `c3`, etc. are Maelstrom's internal clients. Clients send
  requests to servers and expect responses back, via a simple asynchronous [RPC
  protocol](#message-bodies).

The servers implement the "Node" trait, which basically receives an initial state and some logic for the step or state transition function, and also a transimttor channel for communicating with the main thread of the node. This is necessary as it makes implementing the Gossip protocol on the challenge easier.

```rust
pub trait Node<S, Payload, InjectedPayload = ()> {
    fn step(
        &mut self,
        input: Event<Payload, InjectedPayload>,
        output: &mut StdoutLock,
    ) -> anyhow::Result<()>;
    fn from_init(
        state: S,
        init: Init,
        tx: std::sync::mpsc::Sender<Event<Payload, InjectedPayload>>,
    ) -> anyhow::Result<Self>
    where
        Self: Sized;
}

