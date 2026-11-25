fn main() {
    let root = std::env::args().nth(1).unwrap();
    let (mut ok, mut skip, mut total) = (0, 0, 0);
    fn walk(dir: &std::path::Path, f: &mut impl FnMut(&std::path::Path)) {
        if let Ok(rd) = std::fs::read_dir(dir) {
            for e in rd.flatten() {
                let p = e.path();
                if p.is_dir() { walk(&p, f) } else { f(&p) }
            }
        }
    }
    walk(std::path::Path::new(&root), &mut |p| {
        if p.extension()
            .is_some_and(|e| e.to_string_lossy().eq_ignore_ascii_case("3df"))
        {
            total += 1;
            match atb::read_texture_3df(&std::fs::read(p).unwrap()) {
                Ok(_) => ok += 1,
                Err(_) => skip += 1,
            }
        }
    });
    println!("3DF: total={total} parsed={ok} skipped={skip}");
}
