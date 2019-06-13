use ethereum_types::H256 as Hash256;
use std::collections::{BTreeMap, HashMap};
use std::ops::Range;

pub const SKIP_LIST_LEN: usize = 16;

pub type Height = usize;
pub type Slot = u64;

#[derive(Default, Clone)]
pub struct Node {
    pub parent_hash: Option<Hash256>,
    pub children: Vec<Hash256>,
    pub score: u64,
    pub height: Height,
    pub block_hash: Hash256,
}

impl Node {
    fn does_not_have_children(&self) -> bool {
        self.children.is_empty()
    }
}

pub struct Tree {
    store: Store,
    nodes: HashMap<Hash256, Node>,
    root: Hash256,
    slots_at_height: SortedList<Slot>,
    blocks_at_height: HashMap<Height, Vec<Hash256>>,
}

impl Tree {
    pub fn new(root: Hash256, height: Height) -> Self {
        let mut node: Node = Node::default();
        node.height = 0;

        let mut nodes = HashMap::new();
        nodes.insert(root, Node::default());

        let mut blocks_at_height = HashMap::new();
        blocks_at_height.insert(height, vec![root]);

        Self {
            store: Store::default(),
            nodes,
            root,
            slots_at_height: SortedList::new(),
            blocks_at_height,
        }
    }

    pub fn add_node(&mut self, hash: Hash256, block_hash: Hash256) -> Option<()> {
        // TODO: resolve clone.
        let mut prev_in_tree = self
            .find_prev_in_tree(hash, 0..self.slots_at_height.len())?
            .clone();

        let mut node = Node {
            block_hash,
            parent_hash: Some(prev_in_tree.block_hash),
            ..Node::default()
        };

        if prev_in_tree.does_not_have_children() {
            node.parent_hash = Some(prev_in_tree.block_hash);
            prev_in_tree.children.push(hash);
        } else {
            for child_hash in prev_in_tree.children {
                let ancestor_hash = self.find_least_common_ancestor(hash, child_hash)?;
                if ancestor_hash != prev_in_tree.block_hash {
                    let child = self.nodes.get_mut(&child_hash)?;
                    let common_ancestor = Node {
                        block_hash: ancestor_hash,
                        parent_hash: Some(prev_in_tree.block_hash),
                        ..Node::default()
                    };
                    child.parent_hash = Some(common_ancestor.block_hash);
                    node.parent_hash = Some(common_ancestor.block_hash);

                    self.nodes
                        .insert(common_ancestor.block_hash, common_ancestor);
                }
            }
        }

        self.nodes.insert(hash, node);

        Some(())
    }

    fn find_prev_in_tree(&mut self, hash: Hash256, range: Range<Height>) -> Option<&mut Node> {
        if range.len() == 0 || range.end > self.slots_at_height.len() {
            None
        } else {
            let mid_height = range.len() / 2;
            let mid_slot = self.slot_at_height(mid_height)?;
            let mid_ancestor = self.find_ancestor_at_slot(hash, mid_slot)?;

            if self.exists_above_height(hash, mid_height)? {
                if self.exists_between_heights(hash, mid_height..mid_height + 1)? {
                    self.nodes.get_mut(&mid_ancestor)
                } else {
                    self.find_prev_in_tree(hash, mid_height..range.end)
                }
            } else {
                self.find_prev_in_tree(hash, range.start..mid_height)
            }
        }
    }

    fn exists_above_height(&self, hash: Hash256, height: Height) -> Option<bool> {
        let ancestor_at_height = self.find_ancestor_at_height(hash, height)?;
        let blocks_at_height = self.blocks_at_height.get(&height)?;

        Some(blocks_at_height.contains(&ancestor_at_height))
    }

    fn exists_between_heights(&self, hash: Hash256, range: Range<Height>) -> Option<bool> {
        let low_blocks = self.blocks_at_height.get(&range.start)?;
        let high_blocks = self.blocks_at_height.get(&range.end)?;

        let low_ancestor = self.find_ancestor_at_height(hash, range.start)?;
        let high_ancestor = self.find_ancestor_at_height(hash, range.end)?;

        Some(low_blocks.contains(&low_ancestor) && !high_blocks.contains(&high_ancestor))
    }

    fn find_ancestor_at_height(&self, child: Hash256, height: Height) -> Option<Hash256> {
        self.find_ancestor_at_slot(child, self.slot_at_height(height)?)
    }

    fn find_ancestor_at_slot(&self, child: Hash256, slot: Slot) -> Option<Hash256> {
        get_ancestor_hash_at_slot(slot, child, &self.store)
    }

    fn find_least_common_ancestor(&self, a: Hash256, b: Hash256) -> Option<Hash256> {
        find_least_common_ancestor(a, b, &self.store)
    }

    fn slot_at_height(&self, height: Height) -> Option<Slot> {
        self.slots_at_height.nth(height).cloned()
    }
}

pub struct Block {
    pub slot: Slot,
    ancestor_skip_list: [Hash256; SKIP_LIST_LEN],
}

fn get_ancestor_hash_at_slot(slot: Slot, start: Hash256, store: &Store) -> Option<Hash256> {
    let mut block = store.get(&start)?;

    loop {
        if slot >= block.slot {
            break None;
        } else {
            let delta = block.slot - slot;

            if delta > SKIP_LIST_LEN as u64 {
                block = store.get(&block.ancestor_skip_list[SKIP_LIST_LEN - 1])?;
            } else if delta.is_power_of_two() {
                break Some(block.ancestor_skip_list[delta.trailing_zeros() as usize]);
            } else {
                let i = delta.next_power_of_two() - 1;
                block = store.get(&block.ancestor_skip_list[i as usize])?;
            }
        }
    }
}

fn find_least_common_ancestor(a_root: Hash256, b_root: Hash256, store: &Store) -> Option<Hash256> {
    let mut a = store.get(&a_root)?;
    let mut b = store.get(&b_root)?;

    if a.slot > b.slot {
        a = store.get(&get_ancestor_hash_at_slot(b.slot, a_root, store)?)?;
    } else if b.slot > a.slot {
        b = store.get(&get_ancestor_hash_at_slot(a.slot, b_root, store)?)?;
    }

    loop {
        if a.ancestor_skip_list[0] == b.ancestor_skip_list[0] {
            break Some(a.ancestor_skip_list[0]);
        } else if a.slot == 0 || b.slot == 0 {
            break None;
        } else {
            a = store.get(&a.ancestor_skip_list[0])?;
            b = store.get(&b.ancestor_skip_list[0])?;
        }
    }
}

pub type Store = HashMap<Hash256, Block>;

pub struct SortedList<K>(BTreeMap<K, ()>);

impl<K: Ord> SortedList<K> {
    pub fn new() -> Self {
        SortedList(BTreeMap::new())
    }

    pub fn insert(&mut self, key: K) {
        self.0.insert(key, ());
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn nth(&self, n: usize) -> Option<&K> {
        self.0.iter().nth(n).and_then(|(k, _v)| Some(k))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ethereum_types::H256 as Hash256;

    #[test]
    fn new() {
        let genesis_root = Hash256::random();
        let genesis_slot = 0;

        let _t = Tree::new(genesis_root, genesis_slot);
    }
}
