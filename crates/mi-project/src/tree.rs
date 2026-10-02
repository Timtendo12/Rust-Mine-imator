//! The timeline tree: which timeline is whose child, and in what order.
//!
//! Files store the tree as a `parent` id and a `parent_tree_index` per
//! timeline (`project_load_find_save_ids` rebuilds it from those).

use mi_core::SaveId;
use mi_format::project::Timeline;
use std::collections::HashMap;

/// Children of every timeline and of the root, as indices into the
/// project's timeline list.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Tree {
    root: Vec<usize>,
    children: Vec<Vec<usize>>,
    parents: Vec<Option<usize>>,
    order: Vec<usize>,
}

impl Tree {
    /// Builds the tree. Returns it with a description of everything that had
    /// to be repaired.
    ///
    /// The original places each timeline at `parent_tree_index` in its
    /// parent's list; when two timelines claim the same slot one of them
    /// silently disappears from the tree, and a timeline whose ancestors form
    /// a loop is never reachable. Here such timelines are kept: duplicates
    /// are appended after the others and loops are attached to the root.
    pub fn build(timelines: &[Timeline]) -> (Tree, Vec<String>) {
        let mut warnings = Vec::new();
        let index_of: HashMap<&SaveId, usize> = timelines.iter().enumerate().map(|(i, tl)| (&tl.id, i)).collect();

        let mut parents: Vec<Option<usize>> = timelines
            .iter()
            .enumerate()
            .map(|(i, tl)| {
                if tl.parent.is_root() {
                    return None;
                }
                match index_of.get(&tl.parent) {
                    Some(&p) if p != i => Some(p),
                    _ => {
                        warnings.push(format!(
                            "Timeline \"{}\" refers to a parent that does not exist; moved to the top level",
                            tl.name
                        ));
                        None
                    }
                }
            })
            .collect();

        // Break loops: walking up from any timeline must end at the root.
        for start in 0..timelines.len() {
            let mut steps = 0;
            let mut current = parents[start];
            while let Some(p) = current {
                steps += 1;
                if steps > timelines.len() {
                    warnings.push(format!(
                        "Timeline \"{}\" is part of a parent loop; moved to the top level",
                        timelines[start].name
                    ));
                    parents[start] = None;
                    break;
                }
                current = parents[p];
            }
        }

        // Place children in their slots, then the ones without a usable slot.
        let mut slots: Vec<Vec<Option<usize>>> = vec![Vec::new(); timelines.len() + 1];
        let root_slot = timelines.len();
        let mut unplaced = Vec::new();
        for (i, tl) in timelines.iter().enumerate() {
            let list = &mut slots[parents[i].unwrap_or(root_slot)];
            match tl.parent_tree_index {
                Some(slot) if slot >= 0 => {
                    let slot = slot as usize;
                    if list.len() <= slot {
                        list.resize(slot + 1, None);
                    }
                    if list[slot].is_none() {
                        list[slot] = Some(i);
                    } else {
                        unplaced.push(i);
                    }
                }
                _ => unplaced.push(i),
            }
        }
        let mut lists: Vec<Vec<usize>> = slots.into_iter().map(|list| list.into_iter().flatten().collect()).collect();
        for i in unplaced {
            lists[parents[i].unwrap_or(root_slot)].push(i);
        }

        let root = lists.pop().unwrap_or_default();
        let mut tree = Tree { root, children: lists, parents, order: Vec::new() };
        tree.rebuild_order();
        (tree, warnings)
    }

    fn rebuild_order(&mut self) {
        let mut order = Vec::with_capacity(self.children.len());
        let mut stack: Vec<usize> = self.root.iter().rev().copied().collect();
        while let Some(i) = stack.pop() {
            order.push(i);
            stack.extend(self.children[i].iter().rev());
        }
        self.order = order;
    }

    /// Top-level timelines in list order.
    pub fn root(&self) -> &[usize] {
        &self.root
    }

    pub fn children(&self, timeline: usize) -> &[usize] {
        &self.children[timeline]
    }

    pub fn parent(&self, timeline: usize) -> Option<usize> {
        self.parents[timeline]
    }

    /// All timelines depth first, every parent before its children
    /// (`project_timeline_list`).
    pub fn order(&self) -> &[usize] {
        &self.order
    }

    /// Number of ancestors of a timeline.
    pub fn depth(&self, timeline: usize) -> usize {
        std::iter::successors(self.parents[timeline], |&p| self.parents[p]).count()
    }

    /// Position of a timeline among its siblings.
    pub fn index_in_parent(&self, timeline: usize) -> usize {
        let siblings = match self.parents[timeline] {
            Some(p) => &self.children[p],
            None => &self.root,
        };
        siblings.iter().position(|&s| s == timeline).unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mi_core::TlType;
    use mi_format::project::Background;
    use mi_format::ValueSet;

    fn timelines(spec: &[(&str, &str, Option<i64>)]) -> Vec<Timeline> {
        let defaults = ValueSet::project_defaults(&Background::default(), 0.0, 1.0);
        spec.iter()
            .map(|&(id, parent, index)| {
                let mut tl = Timeline::new(SaveId::new(id), TlType::Folder, &defaults);
                tl.name = id.to_owned();
                tl.parent = SaveId::new(parent);
                tl.parent_tree_index = index;
                tl
            })
            .collect()
    }

    #[test]
    fn builds_in_tree_index_order() {
        let tls = timelines(&[
            ("B", "root", Some(1)),
            ("A", "root", Some(0)),
            ("A2", "A", Some(1)),
            ("A1", "A", Some(0)),
            ("A1x", "A1", Some(0)),
        ]);
        let (tree, warnings) = Tree::build(&tls);
        assert!(warnings.is_empty());
        assert_eq!(tree.root(), &[1, 0]);
        assert_eq!(tree.children(1), &[3, 2]);
        let names: Vec<&str> = tree.order().iter().map(|&i| tls[i].name.as_str()).collect();
        assert_eq!(names, ["A", "A1", "A1x", "A2", "B"]);
        assert_eq!(tree.depth(4), 2);
        assert_eq!(tree.parent(4), Some(3));
        assert_eq!(tree.index_in_parent(2), 1);
    }

    #[test]
    fn duplicate_and_missing_slots_keep_every_timeline() {
        let tls = timelines(&[("A", "root", Some(0)), ("B", "root", Some(0)), ("C", "root", None), ("D", "root", Some(5))]);
        let (tree, _) = Tree::build(&tls);
        assert_eq!(tree.root(), &[0, 3, 1, 2]);
        assert_eq!(tree.order().len(), 4);
    }

    #[test]
    fn missing_parents_and_loops_go_to_the_root() {
        let tls = timelines(&[
            ("A", "GONE", Some(0)),
            ("X", "Y", Some(0)),
            ("Y", "X", Some(0)),
            ("S", "S", Some(0)),
        ]);
        let (tree, warnings) = Tree::build(&tls);
        assert_eq!(tree.order().len(), 4, "every timeline is reachable");
        assert!(warnings.len() >= 3, "{warnings:?}");
        assert_eq!(tree.parent(0), None);
        assert_eq!(tree.parent(3), None);
        // One of the two looping timelines keeps the other as its child.
        assert!(tree.parent(1).is_none() || tree.parent(2).is_none());
    }
}
