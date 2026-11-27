//! Lists the node tree of .X files: `xnodes FILE...`.
use parhelia_reef::xfile::{Node, parse};

fn show(n: &Node, depth: usize) {
    if depth > 5 || n.kind == "Material" && depth > 2 {
        return;
    }
    println!(
        "{}{} {} ({} values, {} refs)",
        "  ".repeat(depth),
        n.kind,
        n.name,
        n.values.len(),
        n.refs.len()
    );
    for c in &n.children {
        show(c, depth + 1);
    }
}

fn main() {
    for f in std::env::args().skip(1) {
        let d = std::fs::read(&f).expect("read");
        for n in parse(&d).expect("parse") {
            show(&n, 0);
        }
    }
}
