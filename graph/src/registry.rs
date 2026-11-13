//! The block registry: the palette the UI, the JSON loader, and the validator
//! all use.

use std::collections::HashMap;

use crate::block::{Block, BlockFactory};
use crate::types::BlockSpec;

pub struct Registry {
    factories: Vec<Box<dyn BlockFactory>>,
    specs: HashMap<&'static str, BlockSpec>,
}

impl Registry {
    pub fn new() -> Self {
        Self {
            factories: Vec::new(),
            specs: HashMap::new(),
        }
    }

    /// Register a block type. `B` must be `Default` so the factory can build it.
    pub fn add<B: Block + Default + 'static>(&mut self) {
        let spec = B::default().spec();
        self.specs.insert(spec.id, spec);
        self.factories.push(Box::new(FactoryShim::<B>::new()));
    }

    pub fn get(&self, id: &str) -> Option<&BlockSpec> {
        self.specs.get(id)
    }

    pub fn specs(&self) -> impl Iterator<Item = &BlockSpec> {
        self.specs.values()
    }

    pub fn build(&self, id: &str) -> Option<Box<dyn Block>> {
        self.factories
            .iter()
            .find(|f| f.spec().id == id)
            .map(|f| f.build())
    }
}

impl Default for Registry {
    fn default() -> Self {
        Self::new()
    }
}

// The registry is built once (`builtin_registry`) and only read afterwards:
// every method takes `&self`, and `add` needs `&mut self`. Sharing `&Registry`
// across threads is therefore sound, but the compiler cannot see it because
// `dyn BlockFactory` is only `Send` (the `Nam` block holds `RefCell`s).
// The engine serves the IPC protocol from async tasks, which needs `Sync`.
unsafe impl Sync for Registry {}

struct FactoryShim<B: Block + Default> {
    _marker: std::marker::PhantomData<B>,
}

impl<B: Block + Default> FactoryShim<B> {
    fn new() -> Self {
        Self {
            _marker: std::marker::PhantomData,
        }
    }
}

impl<B: Block + Default + 'static> BlockFactory for FactoryShim<B> {
    fn spec(&self) -> BlockSpec {
        B::default().spec()
    }
    fn build(&self) -> Box<dyn Block> {
        Box::new(B::default())
    }
}
