//! Prints a scene file as an indented tree, eliding long arrays: `dump DATA FILE [FIELD]`.
use timbury::{data::Files, tree};

fn show(n: &tree::Node, depth: usize, out: &mut String) {
    let pad = "  ".repeat(depth);
    out.push_str(&format!("{pad}{} '{}'\n", n.kind, n.name));
    for (k, v) in &n.fields {
        match v {
            tree::Value::Node(c) => {
                out.push_str(&format!("{pad}  .{k}:\n"));
                show(c, depth + 2, out);
            }
            tree::Value::Nodes(cs) => {
                out.push_str(&format!("{pad}  .{k}: [{}]\n", cs.len()));
                for c in cs {
                    show(c, depth + 2, out);
                }
            }
            tree::Value::Floats(f) if f.len() > 16 => out.push_str(&format!(
                "{pad}  .{k}: floats[{}] {:?}..\n",
                f.len(),
                &f[..8]
            )),
            tree::Value::Ints(f) if f.len() > 16 => {
                out.push_str(&format!("{pad}  .{k}: ints[{}] {:?}..\n", f.len(), &f[..8]))
            }
            tree::Value::Bytes(b) => out.push_str(&format!("{pad}  .{k}: bytes[{}]\n", b.len())),
            other => out.push_str(&format!("{pad}  .{k}: {other:?}\n")),
        }
    }
}

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let files = Files::from_dir(std::path::Path::new(&a[0])).unwrap();
    let data = files.get(&a[1]).unwrap();
    let root = if a[1].ends_with(".nmb") {
        tree::read_binary(data).unwrap()
    } else {
        tree::read_text(&String::from_utf8_lossy(data)).unwrap()
    };
    let mut out = String::new();
    match a.get(2) {
        Some(f) => root.nodes(f).iter().for_each(|n| show(n, 0, &mut out)),
        None => show(&root, 0, &mut out),
    }
    print!("{out}");
}
