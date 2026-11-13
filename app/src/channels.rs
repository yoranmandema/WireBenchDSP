//! The channel model: what the UI edits.
//!
//! A **channel** is an ordered strip of blocks. Routing says which other
//! channels feed it. The engine underneath still runs a `Doc` graph, so
//! nothing is lost: [`Rig::to_doc`] lays the channels out as a graph (each
//! channel gets a `Split` node so it can be fed by several sources), and
//! [`Rig::from_doc`] reads that layout back.
//!
//! With one channel this is just a pedalboard. With two it is a parallel rig:
//! channel B's source row has `A` and `In` ticked. A send/return is the same
//! idea with three.

use std::collections::BTreeMap;

use dsp_graph::{Doc, EdgeDoc, NodeDoc};

pub const MAX_CHANNELS: usize = 4;
/// The pedal's input, as a routing source.
pub const IN: &str = "<in>";

#[derive(Clone, Debug)]
pub struct Channel {
    pub id: String,
    pub name: String,
    /// Block instances, in order. A `util.master` is the channel's output and
    /// is never a strip block; it lives in `master_volume` / `master_ceiling`.
    pub blocks: Vec<NodeDoc>,
    /// Source channel ids; `IN` means the pedal input.
    pub sources: Vec<String>,
    /// The channel's output volume, dB.
    pub master_volume: f32,
    /// The channel's limiter ceiling, dB.
    pub master_ceiling: f32,
}

impl Channel {
    pub fn new(id: &str, name: &str) -> Self {
        Self { id: id.into(), name: name.into(), blocks: Vec::new(), sources: vec![IN.into()], master_volume: 0.0, master_ceiling: -1.0 }
    }
}

#[derive(Clone, Debug, Default)]
pub struct Rig {
    pub name: String,
    pub channels: Vec<Channel>,
}

/// The master node id for a channel.
pub fn master_id(ch: &str) -> String {
    format!("{ch}__master")
}

/// Id of the pedal's global output limiter. Reserved: `Rig::from_doc` uses it
/// to tell the output node apart from a channel master, which is also a
/// `util.master`.
pub const OUT_MASTER_ID: &str = "out__master";

impl Rig {
    pub fn single(name: &str, blocks: Vec<NodeDoc>) -> Self {
        let mut c = Channel::new("ch1", "Channel 1");
        c.blocks = blocks;
        c.sources = vec![IN.into()];
        Self { name: name.into(), channels: vec![c] }
    }

    pub fn channel(&self, id: &str) -> Option<&Channel> {
        self.channels.iter().find(|c| c.id == id)
    }

    pub fn channel_mut(&mut self, id: &str) -> Option<&mut Channel> {
        self.channels.iter_mut().find(|c| c.id == id)
    }

    pub fn id_taken(&self, id: &str) -> bool {
        self.channels.iter().any(|c| c.blocks.iter().any(|b| b.id == id))
    }

    /// Add a block to a channel's strip. Returns its id.
    pub fn add_block(&mut self, channel: &str, block: &str, pos: [f32; 2]) -> String {
        let stem = block.rsplit('.').next().unwrap_or(block).to_string();
        self.add_block_as(channel, block, &stem, pos)
    }

    /// As [`Rig::add_block`], but the node is named after the block's display
    /// name rather than the block type's last segment. Picking "Amp" from the
    /// palette should not produce a node called "basic".
    pub fn add_block_as(&mut self, channel: &str, block: &str, name: &str, pos: [f32; 2]) -> String {
        let stem = name.trim().to_lowercase();
        let stem = if stem.is_empty() { block.rsplit('.').next().unwrap_or(block).to_string() } else { stem };
        let mut id = stem.clone();
        let mut k = 2;
        while self.id_taken(&id) {
            id = format!("{stem}{k}");
            k += 1;
        }
        let n = NodeDoc { id: id.clone(), block: block.into(), params: BTreeMap::new(), bypass: false, pos: Some(pos), model: None, };
        self.channel_mut(channel).expect("channel").blocks.push(n);
        id
    }

    pub fn remove_block(&mut self, channel: &str, id: &str) {
        if let Some(c) = self.channel_mut(channel) {
            c.blocks.retain(|b| b.id != id);
        }
    }

    /// Move a block within its strip. `to` is the target index.
    pub fn move_block(&mut self, channel: &str, id: &str, to: usize) {
        if let Some(c) = self.channel_mut(channel) {
            if let Some(from) = c.blocks.iter().position(|b| b.id == id) {
                let b = c.blocks.remove(from);
                let to = to.min(c.blocks.len());
                c.blocks.insert(to, b);
            }
        }
    }

    pub fn add_channel(&mut self) -> Option<String> {
        if self.channels.len() >= MAX_CHANNELS {
            return None;
        }
        let id = format!("ch{}", self.channels.len() + 1);
        self.channels.push(Channel::new(&id, &format!("Channel {}", self.channels.len() + 1)));
        Some(id)
    }

    pub fn remove_channel(&mut self, id: &str) {
        self.channels.retain(|c| c.id != id);
        for c in &mut self.channels {
            c.sources.retain(|s| s != id);
            if c.sources.is_empty() {
                c.sources.push(IN.into());
            }
        }
    }

    // --- graph conversion -------------------------------------------------

    pub fn split_id(ch: &str) -> String {
        format!("{ch}__split")
    }
    /// The output node: volume 0 dB, −1 dB ceiling. Shared by the normal path
    /// and the zero-channel bypass.
    fn out_master() -> NodeDoc {
        NodeDoc {
            id: OUT_MASTER_ID.into(),
            block: "util.master".into(),
            params: [("volume".to_string(), 0.0), ("ceiling".to_string(), -1.0)].into_iter().collect(),
            bypass: false,
            pos: Some([1320.0, 400.0]), model: None, }
    }

    /// Lay the rig out as a graph the executor can compile.
    pub fn to_doc(&self) -> Doc {
        let mut doc = Doc::new(self.name.clone());
        doc.nodes.push(NodeDoc { id: "in".into(), block: "io.input".into(), params: BTreeMap::new(), bypass: false, pos: Some([60.0, 400.0]), model: None, });
        doc.nodes.push(NodeDoc { id: "out".into(), block: "io.output".into(), params: BTreeMap::new(), bypass: false, pos: Some([1400.0, 400.0]), model: None, });
        // A pedalboard with no channels at all is a *bypass*, not a dead pedal.
        // The summer below needs a channel master to feed it, so without this
        // the sum's inputs would all be tied to silence and the pedal would
        // output nothing until a block was added.
        if self.channels.is_empty() {
            doc.nodes.push(Self::out_master());
            doc.edges.push(EdgeDoc { from: ["in".to_string(), "out".into()], to: ["out__master".to_string(), "in".into()] });
            doc.edges.push(EdgeDoc { from: ["out__master".to_string(), "out".into()], to: ["out".to_string(), "in".into()] });
            return doc;
        }
        // One summing node so every channel master has exactly one wire into
        // the output node (which has a single input port).
        let mut sum = NodeDoc { id: "sum".into(), block: "util.sum".into(), params: BTreeMap::new(), bypass: false, pos: Some([1250.0, 400.0]), model: None, };
        sum.params.insert("a".into(), 1.0);
        sum.params.insert("b".into(), 1.0);
        sum.params.insert("c".into(), 1.0);
        sum.params.insert("d".into(), 1.0);
        doc.nodes.push(sum);
        for (ci, c) in self.channels.iter().enumerate() {
            let y = 240.0 + ci as f32 * 150.0;
            let multi = c.sources.len() > 1;
            if multi {
                doc.nodes.push(NodeDoc {
                    id: Self::split_id(&c.id),
                    block: "util.mix".into(),
                    params: BTreeMap::new(),
                    bypass: false,
                    pos: Some([240.0, y]), model: None, });
            }
            for (i, b) in c.blocks.iter().enumerate() {
                let mut b = b.clone();
                b.pos = Some([400.0 + i as f32 * 175.0, y]);
                doc.nodes.push(b);
            }
            let mut master = NodeDoc {
                id: master_id(&c.id),
                block: "util.master".into(),
                params: BTreeMap::new(),
                bypass: false,
                pos: Some([400.0 + c.blocks.len() as f32 * 175.0, y]), model: None, };
            master.params.insert("volume".into(), c.master_volume);
            master.params.insert("ceiling".into(), c.master_ceiling);
            doc.nodes.push(master);

            // The first node of the chain (the split, or the first block).
            let head = if multi {
                Self::split_id(&c.id)
            } else {
                c.blocks.first().map(|b| b.id.clone()).unwrap_or_else(|| master_id(&c.id))
            };
            // Routing edges into the head. (A channel's master is wired to
            // the pedal output by the channel-end edge below.)
            for (k, src) in c.sources.iter().enumerate() {
                let (from_node, from_port) = if src == IN {
                    ("in".to_string(), "out".to_string())
                } else {
                    (master_id(src), "out".to_string())
                };
                let to_port = if multi {
                    if k == 0 { "in_a" } else { "in_b" }
                } else {
                    "in"
                };
                doc.edges.push(EdgeDoc { from: [from_node, from_port], to: [head.clone(), to_port.into()] });
            }
            // The chain itself. With a split, the split feeds the first
            // block; without one, the first block *is* the head and the
            // routing edge above already feeds it.
            if multi {
                let mut prev = head.clone();
                for b in &c.blocks {
                    doc.edges.push(EdgeDoc { from: [prev.clone(), "out".into()], to: [b.id.clone(), "in".into()] });
                    prev = b.id.clone();
                }
            } else {
                for w in c.blocks.windows(2) {
                    doc.edges.push(EdgeDoc { from: [w[0].id.clone(), "out".into()], to: [w[1].id.clone(), "in".into()] });
                }
            }
            let last = if multi {
                head.clone()
            } else {
                c.blocks.last().map(|b| b.id.clone()).unwrap_or_else(|| head.clone())
            };
            // An empty channel's head *is* the master, so the routing edge
            // above already feeds it; adding another edge here would be a
            // self-loop (and a second driver on the master's input).
            if last != master_id(&c.id) {
                doc.edges.push(EdgeDoc { from: [last, "out".into()], to: [master_id(&c.id), "in".into()] });
            }
            let port = ["a", "b", "c", "d"][ci.min(3)];
            doc.edges.push(EdgeDoc { from: [master_id(&c.id), "out".into()], to: ["sum".to_string(), port.into()] });
        }
        // Unused sum inputs are tied to silence via a zero-gain gain node, so
        // the graph's "every input driven" rule holds without a special case.
        for (k, port) in ["a", "b", "c", "d"].iter().enumerate().skip(self.channels.len()) {
            let id = format!("sum_z{k}");
            doc.nodes.push(NodeDoc {
                id: id.clone(),
                block: "util.gain".into(),
                params: [("gain".to_string(), -60.0)].into_iter().collect(),
                bypass: false,
                pos: Some([1150.0, 200.0 + k as f32 * 60.0]), model: None, });
            doc.edges.push(EdgeDoc { from: ["in".to_string(), "out".into()], to: [id.clone(), "in".into()] });
            doc.edges.push(EdgeDoc { from: [id, "out".into()], to: ["sum".to_string(), port.to_string()] });
        }
        // One limiter on the summed output: each channel has its own master,
        // but their sum can still exceed full scale.
        doc.nodes.push(Self::out_master());
        doc.edges.push(EdgeDoc { from: ["sum".to_string(), "out".into()], to: ["out__master".to_string(), "in".into()] });
        doc.edges.push(EdgeDoc { from: ["out__master".to_string(), "out".into()], to: ["out".to_string(), "in".into()] });
        doc
    }

    /// Read a doc back into channels.
    ///
    /// A channel is a chain of blocks ending at a `util.master`. Walking
    /// backwards from the master, the chain stops at the first node whose
    /// input comes from the pedal input, from another channel's master, or
    /// from several sources at once (a summing point). That node is the
    /// channel's first block, and its inputs are the channel's sources.
    pub fn from_doc(doc: &Doc) -> Self {
        let mut rig = Rig { name: doc.name.clone(), channels: Vec::new() };
        // Channel masters are the `util.master` nodes that are not the global
        // output limiter. Hand-written presets wire their masters straight to
        // `out` (there is no summer), while the app's own docs wire them to
        // `sum` and reserve `out__master` for the limiter — so the reserved id
        // is the one thing both shapes agree on.
        let masters: Vec<String> = {
            let mut v: Vec<String> = doc
                .nodes
                .iter()
                .filter(|n| n.block == "util.master" && n.id != OUT_MASTER_ID)
                .map(|n| n.id.clone())
                .collect();
            v.sort();
            v
        };
        for master in masters.iter() {
            let mut blocks: Vec<NodeDoc> = Vec::new();
            let mut cur = master.clone();
            let mut sources: Vec<String> = Vec::new();
            loop {
                let incoming: Vec<&EdgeDoc> = doc.edges.iter().filter(|e| e.to[0] == cur && e.to[1] == "in").collect();
                if incoming.is_empty() {
                    break;
                }
                if incoming.len() > 1 {
                    // A summing point: every input must be a channel source.
                    // The node itself belongs to this channel.
                    if let Some(n) = doc.node(&cur) {
                        if n.block != "util.master" {
                            blocks.push(n.clone());
                        }
                    }
                    sources = incoming.iter().map(|e| src_name(&e.from[0])).collect();
                    break;
                }
                // `cur` is part of this channel's chain (it isn't the
                // master itself, which the loop's start handles).
                if let Some(n) = doc.node(&cur) {
                    if n.block != "util.master" {
                        blocks.push(n.clone());
                    }
                }
                let src = incoming[0].from[0].clone();
                if src == "in" || masters.contains(&src) || src.ends_with("__split") {
                    sources.push(src_name(&src));
                    break;
                }
                cur = src;
            }
            if sources.is_empty() {
                sources.push(IN.into());
            }
            let (master_volume, master_ceiling) = doc
                .node(master)
                .map(|n| (n.params.get("volume").copied().unwrap_or(0.0), n.params.get("ceiling").copied().unwrap_or(-1.0)))
                .unwrap_or((0.0, -1.0));
            blocks.reverse();
            // The channel id is the master's prefix, so a source name in the
            // preset ("amp") matches the channel that owns "amp__master".
            let id = master.strip_suffix("__master").unwrap_or(master).to_string();
            let n = rig.channels.len() + 1;
            rig.channels.push(Channel { id, name: format!("Channel {n}"), blocks, sources, master_volume, master_ceiling });
        }
        rig
    }
}

fn src_name(node: &str) -> String {
    if node == "in" {
        IN.into()
    } else if let Some(s) = node.strip_suffix("__master") {
        s.into()
    } else {
        node.into()
    }
}
