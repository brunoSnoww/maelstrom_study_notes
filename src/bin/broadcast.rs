use std::collections::{HashMap, HashSet};
use std::io::StdoutLock;
use std::time::Duration;

use anyhow::Context;
use rand::Rng;
use serde::{Deserialize, Serialize};

use maelstrom_rust::*;

enum InjectedPayload {
    Gossip,
}

// create an enum that is internally tagged by serde
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(tag = "type")]
#[serde(rename_all = "snake_case")]
pub enum Payload {
    Broadcast {
        message: usize,
    },
    BroadcastOk,
    Read,
    ReadOk {
        messages: Vec<usize>,
    },
    Topology {
        topology: HashMap<String, Vec<String>>,
    },
    TopologyOk,
    Gossip {
        seen: HashSet<usize>,
    },
}

pub struct BroadcastNode {
    pub node: String,
    pub messages: Vec<usize>,
    pub neighbors: Vec<String>,
    pub id: usize,
    pub known: HashMap<String, HashSet<usize>>,
}

impl Node<(), Payload, InjectedPayload> for BroadcastNode {
    fn step(
        &mut self,
        input: Event<Payload, InjectedPayload>,
        output: &mut StdoutLock,
    ) -> anyhow::Result<()> {
        match input {
            Event::Message(input) => {
                let mut reply = input.into_reply(Some(&mut self.id));
                match reply.body.payload {
                    Payload::Broadcast { message } => {
                        self.messages.push(message);
                        reply.body.payload = Payload::BroadcastOk;
                        reply.send(output).context("failed to send broadcast_ok")?
                    }
                    Payload::Read => {
                        reply.body.payload = Payload::ReadOk {
                            messages: self.messages.clone(),
                        };
                        reply.send(output).context("failed to send read_ok")?
                    }
                    Payload::Topology { mut topology } => {
                        reply.body.payload = Payload::TopologyOk;
                        self.neighbors = topology
                            .remove(&self.node)
                            .unwrap_or_else(|| panic!("no topology given to node {}", self.node));
                        reply.send(output).context("failed to send topology_ok")?
                    }
                    Payload::Gossip { seen } => {
                        self.known
                            .get_mut(&reply.dst)
                            .expect("got gossip from unknown source")
                            .extend(seen.iter().copied());
                        self.messages.extend(seen);
                    }
                    _ => {}
                }
            }
            Event::Injected(_) => {
                for n in &self.neighbors {
                    let known_n = &self.known[n];
                    let (already_known, mut notify_of): (HashSet<_>, HashSet<_>) = self
                        .messages
                        .iter()
                        .copied()
                        .partition(|m| known_n.contains(m));

                    let mut rng = rand::thread_rng();
                    let additional_cap = (10 * notify_of.len() / 100) as u32;
                    notify_of.extend(already_known.iter().filter(|_| {
                        rng.gen_ratio(
                            additional_cap.min(already_known.len() as u32),
                            already_known.len() as u32,
                        )
                    }));
                    Message {
                        src: self.node.clone(),
                        dst: n.clone(),
                        body: Body {
                            id: None,
                            in_reply_to: None,
                            payload: Payload::Gossip { seen: notify_of },
                        },
                    }
                    .send(&mut *output)
                    .with_context(|| format!("gossip to {}", n))?;
                }
            }
            _ => {}
            Event::EOF => todo!(),
            _ => panic!("This should never happen"),
        }

        Ok(())
    }
    fn from_init(
        _state: (),
        init: Init,
        tx: std::sync::mpsc::Sender<Event<Payload, InjectedPayload>>,
    ) -> anyhow::Result<Self>
    where
        Self: Sized,
    {
        std::thread::spawn(move || loop {
            std::thread::sleep(Duration::from_millis(300));
            if let Err(_) = tx.send(Event::Injected(InjectedPayload::Gossip)) {
                break;
            }
        });
        Ok(BroadcastNode {
            id: 1,
            node: init.node_id,
            messages: vec![],
            neighbors: vec![],
            known: init
                .node_ids
                .into_iter()
                .map(|nid| (nid, HashSet::new()))
                .collect(),
        })
    }
}

fn main() -> anyhow::Result<()> {
    main_loop::<_, BroadcastNode, _, _>(())
}
