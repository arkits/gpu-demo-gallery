//! Prints the WGSL of a listing: `cargo run -p nv-asm --example wgsl FILE.asm`.
fn main() {
    let path = std::env::args().nth(1).expect("listing");
    let p = nv_asm::parse(&std::fs::read_to_string(path).unwrap()).unwrap();
    let opts = nv_asm::wgsl::Options::default();
    let (src, iface) = match p.stage {
        nv_asm::Stage::Vertex => nv_asm::wgsl::vertex(&p, &opts),
        nv_asm::Stage::Fragment => nv_asm::wgsl::fragment(&p, &opts),
    }
    .unwrap();
    println!("{src}\n// {iface:?}");
}
