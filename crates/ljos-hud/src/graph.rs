//! Trust-graph layout. Positions are computed once from pack data, not per frame.

use std::collections::BTreeMap;

use ljos_cli::{Persona, Trust};

/// Radius of a node disk in unit space (circle radius 1).
pub const NODE_R: f32 = 0.08;
/// Focus-ring width in unit space. Mapped to pixels with the layout scale.
pub const FOCUS_RING_W: f32 = 0.035;

/// One vertex: a persona and/or a trust endpoint.
#[derive(Debug, Clone, PartialEq)]
pub struct GraphNode {
    pub name: String,
    /// `None` when the pack has no persona of this name (hollow disk).
    pub anchor: Option<f64>,
    /// Unit-circle coordinates, origin at the centre, computed from the pack.
    pub x: f32,
    pub y: f32,
}

impl GraphNode {
    /// Missing persona: stroke only, no fill.
    #[must_use]
    pub fn hollow(&self) -> bool {
        self.anchor.is_none()
    }
}

/// One directed trust edge. `from` / `to` index [`GraphLayout::nodes`].
#[derive(Debug, Clone, PartialEq)]
pub struct GraphEdge {
    pub from: usize,
    pub to: usize,
    pub weight: f64,
    pub about: Vec<String>,
}

/// Data-layer geometry. [`paint_ops`] only scales these points onto a frame.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct GraphLayout {
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
}

/// A draw instruction in pixel space. Tests inspect this; iced paints it.
#[derive(Debug, Clone, PartialEq)]
pub enum PaintOp {
    Edge {
        from: (f32, f32),
        to: (f32, f32),
        width: f32,
        dashed: bool,
    },
    Node {
        at: (f32, f32),
        r: f32,
        hollow: bool,
    },
    Ring {
        at: (f32, f32),
        r: f32,
        width: f32,
    },
    Focus {
        at: (f32, f32),
        r: f32,
        width: f32,
    },
    Label {
        at: (f32, f32),
        text: String,
    },
}

impl GraphLayout {
    /// Empty graph: habitat down, or a pack with no personas and no trust.
    pub fn empty() -> Self {
        Self::default()
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Nodes = personas ∪ trust endpoints. Edges = the trust rows.
    ///
    /// Positions sit on a circle sorted by name. That pass runs here, once,
    /// not inside a canvas `draw`.
    pub fn from_pack(personas: &[Persona], trust: &[Trust]) -> Self {
        let mut by_name: BTreeMap<String, Option<f64>> = BTreeMap::new();
        for p in personas {
            let name = p.name.trim();
            if name.is_empty() {
                continue;
            }
            by_name.insert(name.to_string(), Some(p.anchor.clamp(0.0, 1.0)));
        }
        for t in trust {
            for end in [t.from.trim(), t.to.trim()] {
                if end.is_empty() {
                    continue;
                }
                by_name.entry(end.to_string()).or_insert(None);
            }
        }
        let names: Vec<String> = by_name.keys().cloned().collect();
        let n = names.len();
        let nodes: Vec<GraphNode> = names
            .iter()
            .enumerate()
            .map(|(i, name)| {
                let (x, y) = unit_pos(i, n);
                let &anchor = by_name.get(name).expect("name from map");
                GraphNode {
                    name: name.clone(),
                    anchor,
                    x,
                    y,
                }
            })
            .collect();
        let index: BTreeMap<&str, usize> = nodes
            .iter()
            .enumerate()
            .map(|(i, node)| (node.name.as_str(), i))
            .collect();
        let edges = trust
            .iter()
            .filter_map(|t| {
                let from = index.get(t.from.trim())?;
                let to = index.get(t.to.trim())?;
                Some(GraphEdge {
                    from: *from,
                    to: *to,
                    weight: t.weight,
                    about: t.about.clone(),
                })
            })
            .collect();
        Self { nodes, edges }
    }

    /// Stroke width in unit space. Heavier trust is a thicker line.
    pub fn stroke_width(weight: f64) -> f32 {
        0.012 + 0.036 * weight.clamp(0.0, 1.0) as f32
    }

    /// Extra radius in unit space. Stubborn personas (`anchor → 0`) get a
    /// wide ring; a plain DeGroot voter (`anchor = 1`) gets none.
    pub fn ring_extra(anchor: f64) -> f32 {
        0.07 * (1.0 - anchor.clamp(0.0, 1.0)) as f32
    }

    /// Scoped `--about` rows are dashed; an unscoped row is solid.
    pub fn is_dashed(about: &[String]) -> bool {
        about.iter().any(|w| !w.trim().is_empty())
    }

    pub fn walk_next(&self, selected: usize) -> usize {
        let n = self.nodes.len();
        if n == 0 {
            0
        } else {
            selected.saturating_add(1) % n
        }
    }

    pub fn walk_prev(&self, selected: usize) -> usize {
        let n = self.nodes.len();
        if n == 0 {
            0
        } else {
            selected.checked_add(n - 1).unwrap_or(0) % n
        }
    }

    /// Keyboard readout for the selected node. Empty when the graph is empty.
    /// A missing persona (`anchor` is `None`) is `name  hollow` plus incident
    /// edges; it does not invent a DeGroot `anchor=1.00`.
    pub fn readout(&self, selected: usize) -> String {
        let Some(node) = self.nodes.get(selected) else {
            return String::new();
        };
        let head = match node.anchor {
            Some(a) => format!(
                "{}  anchor={:.2}  ring={:.2}",
                node.name,
                a,
                (1.0 - a).clamp(0.0, 1.0)
            ),
            None => format!("{}  hollow", node.name),
        };
        let mut lines = vec![head];
        for edge in &self.edges {
            if edge.from == selected {
                let dest = &self.nodes[edge.to].name;
                lines.push(format!(
                    "  → {dest}  {:.3}{}",
                    edge.weight,
                    about_suffix(&edge.about)
                ));
            } else if edge.to == selected {
                let src = &self.nodes[edge.from].name;
                lines.push(format!(
                    "  ← {src}  {:.3}{}",
                    edge.weight,
                    about_suffix(&edge.about)
                ));
            }
        }
        lines.join("\n")
    }

    /// Map unit positions onto a frame. Layout itself does not rerun.
    pub fn paint_ops(
        &self,
        selected: usize,
        focused: bool,
        width: f32,
        height: f32,
    ) -> Vec<PaintOp> {
        if self.nodes.is_empty() {
            return Vec::new();
        }
        let scale = FrameScale::new(width, height);
        let mut ops = Vec::new();
        for edge in &self.edges {
            if edge.from == edge.to {
                continue;
            }
            let a = &self.nodes[edge.from];
            let b = &self.nodes[edge.to];
            ops.push(PaintOp::Edge {
                from: scale.map(a.x, a.y),
                to: scale.map(b.x, b.y),
                width: Self::stroke_width(edge.weight) * scale.s,
                dashed: Self::is_dashed(&edge.about),
            });
        }
        for (i, node) in self.nodes.iter().enumerate() {
            let at = scale.map(node.x, node.y);
            let r = NODE_R * scale.s;
            ops.push(PaintOp::Node {
                at,
                r,
                hollow: node.hollow(),
            });
            let extra = node.anchor.map(Self::ring_extra).unwrap_or(0.0);
            if extra > 0.001 {
                ops.push(PaintOp::Ring {
                    at,
                    r: (NODE_R + extra) * scale.s,
                    width: (0.018 * scale.s).max(1.0),
                });
            }
            if focused && i == selected {
                let focus_r = (NODE_R + extra + FOCUS_RING_W) * scale.s;
                ops.push(PaintOp::Focus {
                    at,
                    r: focus_r,
                    width: (FOCUS_RING_W * scale.s).max(2.0),
                });
            }
            ops.push(PaintOp::Label {
                at: (at.0, at.1 + r + 4.0),
                text: node.name.clone(),
            });
        }
        ops
    }
}

struct FrameScale {
    cx: f32,
    cy: f32,
    s: f32,
}

impl FrameScale {
    fn new(width: f32, height: f32) -> Self {
        let pad = 48.0;
        let s = (width.min(height) / 2.0 - pad).max(1.0);
        Self {
            cx: width / 2.0,
            cy: height / 2.0,
            s,
        }
    }

    fn map(&self, x: f32, y: f32) -> (f32, f32) {
        (self.cx + x * self.s, self.cy + y * self.s)
    }
}

fn unit_pos(i: usize, n: usize) -> (f32, f32) {
    match n {
        0 => (0.0, 0.0),
        1 => (0.0, 0.0),
        _ => {
            let angle =
                -std::f32::consts::FRAC_PI_2 + (i as f32) * 2.0 * std::f32::consts::PI / (n as f32);
            (angle.cos(), angle.sin())
        }
    }
}

fn about_suffix(about: &[String]) -> String {
    let words: Vec<&str> = about
        .iter()
        .map(|w| w.trim())
        .filter(|w| !w.is_empty())
        .collect();
    if words.is_empty() {
        String::new()
    } else {
        format!("  {}", words.join(","))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn persona(name: &str, anchor: f64) -> Persona {
        Persona {
            runner: None,
            name: name.into(),
            anchor,
            view: "view".into(),
            entities: vec![],
        }
    }

    fn trust(from: &str, to: &str, weight: f64, about: &[&str]) -> Trust {
        Trust {
            from: from.into(),
            to: to.into(),
            weight,
            about: about.iter().map(|s| (*s).to_string()).collect(),
        }
    }

    #[test]
    fn empty_pack_is_empty_canvas() {
        let g = GraphLayout::from_pack(&[], &[]);
        assert!(g.is_empty());
        assert!(g.paint_ops(0, true, 400.0, 400.0).is_empty());
        assert!(g.readout(0).is_empty());
    }

    #[test]
    fn nodes_are_personas_union_trust_endpoints() {
        let g = GraphLayout::from_pack(
            &[persona("alpha", 0.2)],
            &[trust("alpha", "beta", 0.8, &[])],
        );
        let names: Vec<&str> = g.nodes.iter().map(|n| n.name.as_str()).collect();
        assert_eq!(names, ["alpha", "beta"]);
        assert!(!g.nodes[0].hollow());
        assert!(g.nodes[1].hollow(), "missing persona is hollow");
        assert_eq!(g.nodes[1].anchor, None);
        assert_eq!(g.edges.len(), 1);
        assert_eq!(g.edges[0].from, 0);
        assert_eq!(g.edges[0].to, 1);
    }

    #[test]
    fn layout_is_independent_of_frame_size() {
        let g = GraphLayout::from_pack(
            &[persona("a", 0.0), persona("b", 1.0), persona("c", 0.5)],
            &[trust("a", "b", 1.0, &[])],
        );
        let a = g.nodes[0].x;
        let again = GraphLayout::from_pack(
            &[persona("a", 0.0), persona("b", 1.0), persona("c", 0.5)],
            &[trust("a", "b", 1.0, &[])],
        );
        assert_eq!(again.nodes[0].x, a);
        let small = g.paint_ops(0, true, 200.0, 200.0);
        let large = g.paint_ops(0, true, 800.0, 800.0);
        let small_nodes: Vec<(f32, f32)> = small
            .iter()
            .filter_map(|op| match op {
                PaintOp::Node { at, .. } => Some(*at),
                _ => None,
            })
            .collect();
        let large_nodes: Vec<(f32, f32)> = large
            .iter()
            .filter_map(|op| match op {
                PaintOp::Node { at, .. } => Some(*at),
                _ => None,
            })
            .collect();
        assert_eq!(small_nodes.len(), 3);
        assert_eq!(large_nodes.len(), 3);
        // Same unit layout: large is a uniform scale about its own centre.
        let s_cx = 100.0;
        let l_cx = 400.0;
        let s_s = FrameScale::new(200.0, 200.0).s;
        let l_s = FrameScale::new(800.0, 800.0).s;
        let ratio = l_s / s_s;
        for i in 0..3 {
            let dx_s = small_nodes[i].0 - s_cx;
            let dx_l = large_nodes[i].0 - l_cx;
            assert!((dx_l - dx_s * ratio).abs() < 0.05);
        }
    }

    #[test]
    fn stroke_grows_with_weight_ring_grows_as_anchor_falls() {
        assert!(GraphLayout::stroke_width(0.9) > GraphLayout::stroke_width(0.1));
        assert!(GraphLayout::ring_extra(0.0) > GraphLayout::ring_extra(0.5));
        assert_eq!(GraphLayout::ring_extra(1.0), 0.0);
        assert!(GraphLayout::is_dashed(&["hud".into()]));
        assert!(!GraphLayout::is_dashed(&[]));
        assert!(!GraphLayout::is_dashed(&[String::new()]));
    }

    #[test]
    fn paint_ops_mark_scoped_edges_dashed_and_focus_the_walk() {
        let g = GraphLayout::from_pack(
            &[persona("a", 0.0), persona("b", 1.0)],
            &[trust("a", "b", 0.9, &["hud"]), trust("b", "a", 0.2, &[])],
        );
        let ops = g.paint_ops(0, true, 400.0, 400.0);
        let edges: Vec<&PaintOp> = ops
            .iter()
            .filter(|op| matches!(op, PaintOp::Edge { .. }))
            .collect();
        assert_eq!(edges.len(), 2);
        let dashed = edges
            .iter()
            .filter(|op| matches!(op, PaintOp::Edge { dashed: true, .. }))
            .count();
        let solid = edges
            .iter()
            .filter(|op| matches!(op, PaintOp::Edge { dashed: false, .. }))
            .count();
        assert_eq!(dashed, 1);
        assert_eq!(solid, 1);
        let w_hi = edges
            .iter()
            .find_map(|op| match op {
                PaintOp::Edge {
                    width,
                    dashed: true,
                    ..
                } => Some(*width),
                _ => None,
            })
            .unwrap();
        let w_lo = edges
            .iter()
            .find_map(|op| match op {
                PaintOp::Edge {
                    width,
                    dashed: false,
                    ..
                } => Some(*width),
                _ => None,
            })
            .unwrap();
        assert!(w_hi > w_lo);
        assert!(ops.iter().any(|op| matches!(op, PaintOp::Focus { .. })));
        assert!(ops.iter().any(|op| matches!(op, PaintOp::Ring { .. })));
        let unfocused = g.paint_ops(0, false, 400.0, 400.0);
        assert!(!unfocused
            .iter()
            .any(|op| matches!(op, PaintOp::Focus { .. })));
    }

    #[test]
    fn keyboard_walk_wraps_and_readout_names_incident_edges() {
        let g = GraphLayout::from_pack(
            &[persona("a", 0.4), persona("b", 0.6)],
            &[trust("a", "b", 0.5, &["seat"])],
        );
        assert_eq!(g.walk_next(0), 1);
        assert_eq!(g.walk_next(1), 0);
        assert_eq!(g.walk_prev(0), 1);
        let text = g.readout(0);
        assert!(text.contains("a  anchor=0.40  ring=0.60"));
        assert!(text.contains("→ b"));
        assert!(text.contains("seat"));
    }

    #[test]
    fn missing_persona_is_hollow_disk() {
        let g = GraphLayout::from_pack(
            &[persona("alpha", 0.2)],
            &[trust("alpha", "beta", 0.8, &[])],
        );
        assert!(g.nodes[1].hollow());
        assert!(!g.nodes[0].hollow());
        let ops = g.paint_ops(0, true, 400.0, 400.0);
        let hollow = ops
            .iter()
            .filter(|op| matches!(op, PaintOp::Node { hollow: true, .. }))
            .count();
        let filled = ops
            .iter()
            .filter(|op| matches!(op, PaintOp::Node { hollow: false, .. }))
            .count();
        assert_eq!(hollow, 1);
        assert_eq!(filled, 1);
        let text = g.readout(1);
        assert_eq!(text.lines().next(), Some("beta  hollow"));
        assert!(
            !text.contains("anchor="),
            "hollow readout must not invent DeGroot"
        );
        assert!(text.contains("← alpha"));
    }

    #[test]
    fn habitat_down_must_not_invent_nodes() {
        let g = GraphLayout::empty();
        assert!(g.nodes.is_empty());
        assert!(g.edges.is_empty());
        assert!(g.paint_ops(0, true, 800.0, 600.0).is_empty());
    }
}
