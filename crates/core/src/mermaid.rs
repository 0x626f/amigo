//! A native parser for Mermaid's basic flowchart notation. No scripts or remote content run.
use std::collections::{BTreeMap, BTreeSet, VecDeque};
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Direction {
    Down,
    Up,
    Right,
    Left,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Shape {
    Rectangle,
    Rounded,
    Diamond,
    Circle,
    Stadium,
    Database,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Line {
    Solid,
    Dotted,
    Thick,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Node {
    pub id: String,
    pub label: String,
    pub shape: Shape,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Edge {
    pub from: usize,
    pub to: usize,
    pub label: String,
    pub start_arrow: bool,
    pub end_arrow: bool,
    pub line: Line,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Group {
    pub title: String,
    pub nodes: Vec<usize>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Flowchart {
    pub direction: Direction,
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
    pub groups: Vec<Group>,
}
pub fn is_mermaid(language: &str) -> bool {
    language
        .split_whitespace()
        .next()
        .is_some_and(|s| s.eq_ignore_ascii_case("mermaid"))
}
pub fn parse(source: &str) -> Result<Flowchart, String> {
    if source.len() > 256_000 {
        return Err("Flowchart source is too large.".into());
    }
    let statements = statements(source)?;
    let Some(header) = statements.first() else {
        return Err("Start with flowchart TD or flowchart LR.".into());
    };
    let mut header = header.split_whitespace();
    if !matches!(header.next(), Some("flowchart" | "graph")) {
        return Err("Only Mermaid flowchart and graph diagrams are supported.".into());
    }
    let direction = match header.next() {
        Some("TD" | "TB") => Direction::Down,
        Some("BT") => Direction::Up,
        Some("LR") => Direction::Right,
        Some("RL") => Direction::Left,
        _ => return Err("Use flowchart TD, TB, BT, LR, or RL.".into()),
    };
    if header.next().is_some() {
        return Err("Put nodes and connections after the flowchart direction on a new line or after a semicolon.".into());
    }
    let mut graph = Flowchart {
        direction,
        nodes: Vec::new(),
        edges: Vec::new(),
        groups: Vec::new(),
    };
    let mut ids = BTreeMap::new();
    let mut groups = Vec::new();
    for (number, statement) in statements.iter().enumerate().skip(1) {
        let result = (|| -> Result<(), String> {
            let mut rest = statement.trim();
            if rest == "end" {
                groups.pop().ok_or("Unexpected end outside a subgraph.")?;
                return Ok(());
            }
            if let Some(label) = rest.strip_prefix("subgraph ") {
                let title = label
                    .find('[')
                    .and_then(|i| label.strip_suffix(']').map(|end| &end[i + 1..]))
                    .unwrap_or(label);
                graph.groups.push(Group {
                    title: label_text(title),
                    nodes: Vec::new(),
                });
                groups.push(graph.groups.len() - 1);
                return Ok(());
            }
            if rest.starts_with("direction ") {
                return Err("Directions inside subgraphs are not supported yet.".into());
            }
            let mut from = nodes(&mut rest, &mut graph, &mut ids, &groups)?;
            while !rest.trim().is_empty() {
                let (start_arrow, end_arrow, line, label) = edge(&mut rest)?;
                let to = nodes(&mut rest, &mut graph, &mut ids, &groups)?;
                for a in &from {
                    for b in &to {
                        graph.edges.push(Edge {
                            from: *a,
                            to: *b,
                            label: label.clone(),
                            start_arrow,
                            end_arrow,
                            line,
                        });
                        if graph.edges.len() > 2048 {
                            return Err("Flowchart has too many connections.".into());
                        }
                    }
                }
                from = to;
            }
            Ok(())
        })();
        result.map_err(|error| format!("Statement {}: {error}", number + 1))?;
    }
    if !groups.is_empty() {
        return Err("A subgraph is missing its closing end.".into());
    }
    if graph.nodes.is_empty() {
        return Err("Add at least one node to the flowchart.".into());
    }
    Ok(graph)
}
fn statements(source: &str) -> Result<Vec<String>, String> {
    let mut result = Vec::new();
    let mut current = String::new();
    let mut stack = Vec::new();
    let mut quote = false;
    let mut comment = false;
    let mut chars = source.chars().peekable();
    while let Some(c) = chars.next() {
        if comment {
            if c != '\n' {
                continue;
            }
            comment = false;
        }
        if c == '"' {
            quote = !quote;
            current.push(c);
            continue;
        }
        if !quote {
            if stack.is_empty() && c == '%' && chars.peek() == Some(&'%') {
                comment = true;
                continue;
            }
            match c {
                '[' | '(' | '{' => stack.push(c),
                ']' | ')' | '}' => {
                    let expected = match c {
                        ']' => '[',
                        ')' => '(',
                        _ => '{',
                    };
                    if stack.pop() != Some(expected) {
                        return Err("Unbalanced node shape delimiters.".into());
                    }
                }
                '\n' | ';' if stack.is_empty() => {
                    if !current.trim().is_empty() {
                        result.push(current.trim().to_owned());
                    }
                    current.clear();
                    continue;
                }
                _ => (),
            }
        }
        current.push(c);
    }
    if quote || !stack.is_empty() {
        return Err("A node label has an unclosed quote or shape.".into());
    }
    if !current.trim().is_empty() {
        result.push(current.trim().to_owned());
    }
    Ok(result)
}
fn label_text(label: &str) -> String {
    label
        .trim()
        .trim_matches('"')
        .trim_matches('`')
        .replace("<br/>", "\n")
        .replace("<br />", "\n")
        .replace("<br>", "\n")
}
fn nodes(
    rest: &mut &str,
    graph: &mut Flowchart,
    ids: &mut BTreeMap<String, usize>,
    groups: &[usize],
) -> Result<Vec<usize>, String> {
    let mut result = Vec::new();
    loop {
        *rest = rest.trim_start();
        let end = rest
            .char_indices()
            .find(|(_, c)| !c.is_alphanumeric() && *c != '_')
            .map_or(rest.len(), |(i, _)| i);
        if end == 0 {
            return Err(format!(
                "Expected a node ID near {}",
                rest.chars().take(35).collect::<String>()
            ));
        }
        let id = rest[..end].to_owned();
        *rest = &rest[end..];
        if matches!(
            id.as_str(),
            "classDef" | "class" | "style" | "click" | "linkStyle" | "direction"
        ) {
            return Err(format!(
                "The {id} directive is not supported by the native flowchart preview."
            ));
        }
        *rest = rest.trim_start();
        if ["[[", "{{", "(((", "[/", "[\\"]
            .iter()
            .any(|prefix| rest.starts_with(prefix))
        {
            return Err("This node shape is not supported by the native flowchart preview.".into());
        }
        let mut shape = Shape::Rectangle;
        let mut label = None;
        for (open, close, kind) in [
            ("((", "))", Shape::Circle),
            ("([", "])", Shape::Stadium),
            ("[(", ")]", Shape::Database),
            ("[", "]", Shape::Rectangle),
            ("(", ")", Shape::Rounded),
            ("{", "}", Shape::Diamond),
        ] {
            if rest.starts_with(open) {
                let body = &rest[open.len()..];
                let mut quoted = false;
                let mut depth = 0;
                let mut finish = None;
                for (i, c) in body.char_indices() {
                    if c == '"' {
                        quoted = !quoted;
                    }
                    if !quoted {
                        if depth == 0 && body[i..].starts_with(close) {
                            finish = Some(i);
                            break;
                        }
                        if c == open.chars().next().unwrap() {
                            depth += 1;
                        }
                        if c == close.chars().next().unwrap() && depth > 0 {
                            depth -= 1;
                        }
                    }
                }
                let end = finish.ok_or("Unclosed node label.")?;
                label = Some(label_text(&body[..end]));
                shape = kind;
                *rest = &body[end + close.len()..];
                break;
            }
        }
        let index = if let Some(index) = ids.get(&id).copied() {
            if let Some(label) = label {
                graph.nodes[index].label = label;
                graph.nodes[index].shape = shape;
            }
            index
        } else {
            if graph.nodes.len() >= 512 {
                return Err("Flowchart has too many nodes.".into());
            }
            let index = graph.nodes.len();
            ids.insert(id.clone(), index);
            graph.nodes.push(Node {
                label: label.unwrap_or_else(|| id.clone()),
                id,
                shape,
            });
            index
        };
        for group in groups {
            if !graph.groups[*group].nodes.contains(&index) {
                graph.groups[*group].nodes.push(index);
            }
        }
        result.push(index);
        *rest = rest.trim_start();
        if let Some(tail) = rest.strip_prefix('&') {
            *rest = tail;
        } else {
            break;
        }
    }
    Ok(result)
}
fn edge(rest: &mut &str) -> Result<(bool, bool, Line, String), String> {
    *rest = rest.trim_start();
    for (prefix, suffix, start, line) in [
        ("<--", "-->", true, Line::Solid),
        ("--", "-->", false, Line::Solid),
        ("-.", ".->", false, Line::Dotted),
        ("==", "==>", false, Line::Thick),
    ] {
        if let Some(tail) = rest.strip_prefix(prefix)
            && !tail.starts_with(['>', '-', '.', '='])
            && let Some(end) = tail.find(suffix)
        {
            let label = &tail[..end];
            if !label.contains(['[', ']', '{', '}']) && !label.trim().is_empty() {
                *rest = &tail[end + suffix.len()..];
                return Ok((start, true, line, label_text(label)));
            }
        }
    }
    for (token, start, end, line) in [
        ("<-.->", true, true, Line::Dotted),
        ("<-->", true, true, Line::Solid),
        ("<==>", true, true, Line::Thick),
        ("-.->", false, true, Line::Dotted),
        ("-->", false, true, Line::Solid),
        ("==>", false, true, Line::Thick),
        ("---", false, false, Line::Solid),
        ("-.-", false, false, Line::Dotted),
        ("===", false, false, Line::Thick),
        ("<--", true, false, Line::Solid),
    ] {
        if let Some(tail) = rest.strip_prefix(token) {
            *rest = tail.trim_start();
            let mut label = String::new();
            if let Some(tail) = rest.strip_prefix('|') {
                let (value, tail) = tail
                    .split_once('|')
                    .ok_or("Unclosed connection label; add |.")?;
                label = label_text(value);
                *rest = tail;
            }
            return Ok((start, end, line, label));
        }
    }
    Err(format!(
        "Unsupported connection near {}",
        rest.chars().take(35).collect::<String>()
    ))
}
impl Flowchart {
    /// Stable breadth-first ranks; visited nodes break cycles without changing edge direction.
    pub fn ranks(&self) -> Vec<(usize, usize)> {
        let mut ranks = vec![None; self.nodes.len()];
        let mut queue = VecDeque::new();
        let mut incoming = BTreeSet::new();
        let endpoints = |edge: &Edge| {
            if edge.start_arrow && !edge.end_arrow {
                (edge.to, edge.from)
            } else {
                (edge.from, edge.to)
            }
        };
        for edge in &self.edges {
            let (from, to) = endpoints(edge);
            if from != to {
                incoming.insert(to);
            }
        }
        for (i, rank) in ranks.iter_mut().enumerate() {
            if !incoming.contains(&i) {
                *rank = Some(0);
                queue.push_back(i);
            }
        }
        let mut next = 0;
        loop {
            while let Some(node) = queue.pop_front() {
                for edge in self.edges.iter().filter(|edge| endpoints(edge).0 == node) {
                    let to = endpoints(edge).1;
                    if ranks[to].is_none() {
                        ranks[to] = Some(ranks[node].unwrap() + 1);
                        queue.push_back(to);
                    }
                }
            }
            if let Some(i) = ranks.iter().position(Option::is_none) {
                ranks[i] = Some(next);
                queue.push_back(i);
                next += 1;
            } else {
                break;
            }
        }
        let mut rows = BTreeMap::new();
        ranks
            .into_iter()
            .map(|rank| {
                let rank = rank.unwrap();
                let slot = rows.entry(rank).or_insert(0);
                let result = (rank, *slot);
                *slot += 1;
                result
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    pub const HARDWARE: &str = r#"flowchart TD
OS[Приложения / ОС: Linux, Windows, AI] --> CPU[CPU: ядра, L1/L2/L3, контроллеры]
CPU <--> RAM[RAM: DDR / ECC]
CPU <-- PCIe --> GPU[GPU: вычислительные блоки]
GPU <--> VRAM[VRAM: GDDR / HBM]
CPU <--> BUS[PCIe / системные шины]
BUS --> SSD[NVMe / SSD]
BUS --> CHIP[Чипсет / I/O]
BUS --> NIC[NIC / сеть]
PLATFORM[Платформа: плата, VRM, UEFI, питание] -.-> CPU
PL"#;
    #[test]
    fn hardware_example_keeps_cyrillic_labels_arrows_and_pcie_label() {
        let graph = parse(HARDWARE).unwrap();
        assert_eq!(graph.nodes.len(), 11);
        assert_eq!(graph.edges.len(), 9);
        assert!(graph.nodes[0].label.starts_with("Приложения"));
        assert!(graph.edges[1].start_arrow && graph.edges[1].end_arrow);
        assert_eq!(graph.edges[2].label, "PCIe");
        assert!(graph.edges[2].start_arrow);
        assert_eq!(graph.edges[8].line, Line::Dotted);
        assert_eq!(graph.ranks().len(), graph.nodes.len());
    }
    #[test]
    fn supports_shapes_chains_branching_and_subgraphs() {
        let graph=parse("graph LR; subgraph cluster[My group]; A(Start) -->|next| B{Choose}; B --> C((End)) & D[Other]; end").unwrap();
        assert_eq!(graph.edges.len(), 3);
        assert_eq!(graph.groups[0].nodes.len(), 4);
        assert_eq!(graph.nodes[1].shape, Shape::Diamond);
        assert_eq!(graph.edges[0].label, "next");
    }
    #[test]
    fn malformed_or_unsupported_syntax_returns_an_error() {
        for source in [
            "sequenceDiagram",
            "flowchart TD\nA[broken",
            "flowchart TD\nA -->",
            "flowchart TD\nstyle A fill:red",
        ] {
            assert!(parse(source).is_err(), "{source}");
        }
    }
    #[test]
    fn reverse_arrows_keep_their_logical_direction_in_layout() {
        let graph = parse("flowchart TD\nA <-- B").unwrap();
        let ranks = graph.ranks();
        assert!(ranks[1].0 < ranks[0].0);
    }
}
