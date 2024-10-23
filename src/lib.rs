use std::io::{BufRead, StdoutLock, Write};

use anyhow::{Context, Error};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone)]
pub enum Event<Payload, InjectedPayload = ()> {
    Message(Message<Payload>),
    Injected(InjectedPayload),
    EOF,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Init {
    pub node_id: String,
    pub node_ids: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Message<Payload> {
    pub src: String,
    #[serde(rename = "dest")]
    pub dst: String,
    pub body: Body<Payload>,
}

impl<Payload> Message<Payload> {
    pub fn into_reply(self, id: Option<&mut usize>) -> Self {
        Self {
            src: self.dst,
            dst: self.src,
            body: Body {
                id: id.map(|id| {
                    let mid = *id;
                    *id += 1;
                    mid
                }),
                in_reply_to: self.body.id,
                payload: self.body.payload,
            },
        }
    }

    pub fn send(&self, output: &mut impl Write) -> anyhow::Result<()>
    where
        Payload: Serialize,
    {
        serde_json::to_writer(&mut *output, self)
            .context("serialize response message")
            .expect("TODO: panic message");
        output.write_all(b"\n").context("write trailing newline")?;
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
#[serde(rename_all = "snake_case")]
enum InitPayload {
    Init(Init),
    InitOk,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Body<Payload> {
    // the two first are shared across all Body
    #[serde(rename = "msg_id")]
    pub id: Option<usize>,
    pub in_reply_to: Option<usize>,
    #[serde(flatten)]
    pub payload: Payload,
}

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

pub fn main_loop<S, N, P, IP>(initial_state: S) -> anyhow::Result<()>
where
    N: Node<S, P, IP>,
    P: DeserializeOwned + Send + 'static,
    IP: Send + 'static,
{
    let stdin = std::io::stdin().lock();
    let mut stdin = stdin.lines();
    let mut stdout = std::io::stdout().lock();
    let init_msg: Message<InitPayload> =
        serde_json::from_str(&stdin.next().expect("no init received").context("fuck")?)
            .context("init message cannot be deserialized")?;
    let InitPayload::Init(init) = init_msg.body.payload else {
        panic!("first message should be init");
    };

    let (tx, rx) = std::sync::mpsc::channel();
    let mut node =
        N::from_init(initial_state, init, tx.clone()).context("node initialization failed")?;

    let reply = Message {
        src: init_msg.dst,
        dst: init_msg.src,
        body: Body {
            id: Some(0),
            in_reply_to: init_msg.body.id,
            payload: InitPayload::InitOk,
        },
    };

    reply.send(&mut stdout).context(" send response to init")?;
    drop(stdin);
    let ih = std::thread::spawn(move || {
        let stdin = std::io::stdin().lock();
        for line in stdin.lines() {
            let line = line.context("Failed to read from Maelstrom input")?;
            let input: Message<P> =
                serde_json::from_str(&line).context("Maelstrom input could not be deserialized")?;
            if let Err(_) = tx.send(Event::Message(input)) {
                return Ok::<(), Error>(());
            }
        }
        let _ = tx.send(Event::EOF);
        Ok(())
    });

    for input in rx {
        node.step(input, &mut stdout)
            .context("Node step function failed")?;
    }

    ih.join().expect("error on joining")?;
    Ok(())
}
