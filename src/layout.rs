use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// The divider's axis: vertical places panes side by side.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Axis {
    Horizontal,
    Vertical,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    Left,
    Right,
    Up,
    Down,
}

/// Geometry owns identifiers only; terminals live independently of this tree.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum SplitTree {
    Leaf(String),
    Split {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        axis: Axis,
        ratio: f32,
        first: Box<SplitTree>,
        second: Box<SplitTree>,
    },
}

fn split_id() -> String {
    let mut bytes = [0u8; 16];
    if let Ok(mut file) = std::fs::File::open("/dev/urandom") {
        use std::io::Read;
        let _ = file.read_exact(&mut bytes);
    }
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    format!(
        "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        bytes[0],
        bytes[1],
        bytes[2],
        bytes[3],
        bytes[4],
        bytes[5],
        bytes[6],
        bytes[7],
        bytes[8],
        bytes[9],
        bytes[10],
        bytes[11],
        bytes[12],
        bytes[13],
        bytes[14],
        bytes[15]
    )
}

impl SplitTree {
    pub fn new(id: impl Into<String>) -> Self {
        Self::Leaf(id.into())
    }

    pub fn leaves(&self) -> Vec<String> {
        match self {
            Self::Leaf(id) => vec![id.clone()],
            Self::Split { first, second, .. } => {
                let mut ids = first.leaves();
                ids.extend(second.leaves());
                ids
            }
        }
    }

    fn contains(&self, id: &str) -> bool {
        match self {
            Self::Leaf(leaf) => leaf == id,
            Self::Split { first, second, .. } => first.contains(id) || second.contains(id),
        }
    }

    /// Call at the persistence trust boundary before restoring any terminals.
    pub fn validate(&self) -> bool {
        fn visit(node: &SplitTree, ids: &mut HashSet<String>, depth: usize) -> bool {
            if depth > 64 {
                return false;
            }
            match node {
                SplitTree::Leaf(id) => !id.is_empty() && ids.insert(id.clone()),
                SplitTree::Split {
                    ratio,
                    first,
                    second,
                    ..
                } => {
                    ratio.is_finite()
                        && (0.2..=0.8).contains(ratio)
                        && visit(first, ids, depth + 1)
                        && visit(second, ids, depth + 1)
                }
            }
        }
        visit(self, &mut HashSet::new(), 0)
    }

    pub fn split(&mut self, target: &str, new_id: impl Into<String>, axis: Axis) -> bool {
        let new_id = new_id.into();
        if new_id.is_empty() || self.contains(&new_id) {
            return false;
        }
        fn insert(node: &mut SplitTree, target: &str, new_id: &str, axis: Axis) -> bool {
            match node {
                SplitTree::Leaf(id) if id == target => {
                    *node = SplitTree::Split {
                        id: Some(split_id()),
                        axis,
                        ratio: 0.5,
                        first: Box::new(SplitTree::Leaf(id.clone())),
                        second: Box::new(SplitTree::Leaf(new_id.to_owned())),
                    };
                    true
                }
                SplitTree::Leaf(_) => false,
                SplitTree::Split { first, second, .. } => {
                    insert(first, target, new_id, axis) || insert(second, target, new_id, axis)
                }
            }
        }
        insert(self, target, &new_id, axis)
    }

    /// Swift `TerminalSplitTree.close` focus: next leaf in pre-close order, else last remaining.
    pub fn focus_after_close(&self, id: &str) -> Option<String> {
        let leaves = self.leaves();
        let index = leaves.iter().position(|leaf| leaf == id)?;
        if index + 1 < leaves.len() {
            Some(leaves[index + 1].clone())
        } else {
            leaves.into_iter().rev().find(|leaf| leaf != id)
        }
    }

    /// The owner decides what closing the final terminal means.
    pub fn remove(&mut self, id: &str) -> bool {
        match self {
            Self::Leaf(_) => false,
            Self::Split { first, second, .. } => {
                if matches!(first.as_ref(), Self::Leaf(leaf) if leaf == id) {
                    *self = second.as_ref().clone();
                    true
                } else if matches!(second.as_ref(), Self::Leaf(leaf) if leaf == id) {
                    *self = first.as_ref().clone();
                    true
                } else {
                    first.remove(id) || second.remove(id)
                }
            }
        }
    }

    pub fn swap(&mut self, a: &str, b: &str) -> bool {
        if a == b || !self.contains(a) || !self.contains(b) {
            return false;
        }
        fn visit(node: &mut SplitTree, a: &str, b: &str) {
            match node {
                SplitTree::Leaf(id) if id == a => *id = b.to_owned(),
                SplitTree::Leaf(id) if id == b => *id = a.to_owned(),
                SplitTree::Leaf(_) => {}
                SplitTree::Split { first, second, .. } => {
                    visit(first, a, b);
                    visit(second, a, b);
                }
            }
        }
        visit(self, a, b);
        true
    }

    /// Resize the closest ancestor sharing the requested divider axis.
    pub fn resize(&mut self, id: &str, requested: Axis, grow: bool) -> bool {
        match self {
            Self::Leaf(_) => false,
            Self::Split {
                axis,
                ratio,
                first,
                second,
                ..
            } => {
                let in_first = first.contains(id);
                if !in_first && !second.contains(id) {
                    return false;
                }
                let child = if in_first { first } else { second };
                if child.resize(id, requested, grow) {
                    return true;
                }
                if *axis != requested {
                    return false;
                }
                *ratio = (*ratio + if grow == in_first { 0.05 } else { -0.05 }).clamp(0.2, 0.8);
                true
            }
        }
    }

    fn weight(&self, along: Axis) -> f32 {
        match self {
            Self::Split {
                axis,
                first,
                second,
                ..
            } if *axis == along => first.weight(along) + second.weight(along),
            _ => 1.0,
        }
    }

    pub fn equalize(&mut self) {
        if let Self::Split {
            axis,
            ratio,
            first,
            second,
            ..
        } = self
        {
            let a = first.weight(*axis);
            // ponytail: >5 aligned panes retain 20–80% safety; add minimum-pixel constraints for exact equality.
            *ratio = (a / (a + second.weight(*axis))).clamp(0.2, 0.8);
            first.equalize();
            second.equalize();
        }
    }

    pub fn focus_neighbor(&self, id: &str, direction: Direction) -> Option<String> {
        fn rects<'a>(node: &'a SplitTree, rect: [f32; 4], out: &mut Vec<(&'a str, [f32; 4])>) {
            match node {
                SplitTree::Leaf(id) => out.push((id, rect)),
                SplitTree::Split {
                    axis,
                    ratio,
                    first,
                    second,
                    ..
                } => {
                    let mut a = rect;
                    let mut b = rect;
                    let i = if *axis == Axis::Vertical { 0 } else { 1 };
                    a[i + 2] *= ratio;
                    b[i] += a[i + 2];
                    b[i + 2] -= a[i + 2];
                    rects(first, a, out);
                    rects(second, b, out);
                }
            }
        }
        let mut panes = Vec::new();
        rects(self, [0.0, 0.0, 1.0, 1.0], &mut panes);
        let source = panes.iter().find(|(key, _)| *key == id)?.1;
        let distance = |r: &[f32; 4]| {
            (r[0] + r[2] / 2.0 - source[0] - source[2] / 2.0).powi(2)
                + (r[1] + r[3] / 2.0 - source[1] - source[3] / 2.0).powi(2)
        };
        panes
            .iter()
            .filter(|(key, r)| {
                if *key == id {
                    return false;
                }
                let overlap_y = r[1] + r[3] > source[1] && r[1] < source[1] + source[3];
                let overlap_x = r[0] + r[2] > source[0] && r[0] < source[0] + source[2];
                match direction {
                    Direction::Left => r[0] + r[2] <= source[0] + 0.000001 && overlap_y,
                    Direction::Right => r[0] >= source[0] + source[2] - 0.000001 && overlap_y,
                    Direction::Up => r[1] + r[3] <= source[1] + 0.000001 && overlap_x,
                    Direction::Down => r[1] >= source[1] + source[3] - 0.000001 && overlap_x,
                }
            })
            .min_by(|(_, a), (_, b)| distance(a).total_cmp(&distance(b)))
            .map(|(key, _)| (*key).to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nested_layout_preserves_identity_and_direction() {
        let mut tree = SplitTree::new("a");
        assert!(tree.split("a", "b", Axis::Vertical));
        assert!(tree.split("b", "c", Axis::Horizontal));
        assert!(!tree.split("a", "c", Axis::Vertical));
        assert_eq!(
            tree.focus_neighbor("b", Direction::Down).as_deref(),
            Some("c")
        );
        assert_eq!(
            tree.focus_neighbor("c", Direction::Left).as_deref(),
            Some("a")
        );
        assert!(tree.resize("b", Axis::Horizontal, true));
        tree.equalize();
        assert!(
            matches!(&tree, SplitTree::Split { ratio, second, .. } if *ratio == 0.5 && matches!(second.as_ref(), SplitTree::Split { ratio, .. } if *ratio == 0.5))
        );
        assert!(tree.swap("a", "c"));
        assert_eq!(tree.leaves(), ["c", "b", "a"]);
        assert!(tree.remove("b"));
        assert_eq!(tree.leaves(), ["c", "a"]);
        assert!(tree.validate());
        assert_eq!(
            serde_json::from_str::<SplitTree>(&serde_json::to_string(&tree).unwrap()).unwrap(),
            tree
        );
        assert!(tree.remove("a"));
        assert!(!tree.remove("c"));
        assert_eq!(tree.leaves(), ["c"]);
        assert!(
            !SplitTree::Split {
                id: None,
                axis: Axis::Vertical,
                ratio: 0.5,
                first: Box::new(SplitTree::new("x")),
                second: Box::new(SplitTree::new("x"))
            }
            .validate()
        );
        let legacy: SplitTree = serde_json::from_str(
            r#"{"Split":{"axis":"Vertical","ratio":0.5,"first":{"Leaf":"a"},"second":{"Leaf":"b"}}}"#,
        )
        .unwrap();
        assert!(matches!(legacy, SplitTree::Split { id: None, .. }));
        assert!(matches!(&tree, SplitTree::Split { id: Some(_), .. }) || tree.validate());
    }

    #[test]
    fn focus_after_close_matches_swift_next_then_last() {
        let leaf = SplitTree::new("a");
        assert_eq!(leaf.focus_after_close("a"), None);
        assert!(!SplitTree::new("a").remove("a"));

        let mut two = SplitTree::new("a");
        assert!(two.split("a", "b", Axis::Vertical));
        assert_eq!(two.focus_after_close("a").as_deref(), Some("b"));
        assert_eq!(two.focus_after_close("b").as_deref(), Some("a"));
        assert!(two.remove("a"));
        assert_eq!(two.leaves(), ["b"]);

        let mut nested = SplitTree::new("a");
        assert!(nested.split("a", "b", Axis::Vertical));
        assert!(nested.split("b", "c", Axis::Horizontal));
        assert_eq!(nested.leaves(), ["a", "b", "c"]);
        assert_eq!(nested.focus_after_close("b").as_deref(), Some("c"));
        assert_eq!(nested.focus_after_close("c").as_deref(), Some("b"));
    }
}
