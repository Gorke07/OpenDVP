//! Write a minimal ES2 `.dvp` (LD X0 / OUT Y0 / END): `cargo run --example write_dvp -- OUT.dvp`
fn main() {
    let path = std::env::args().nth(1).expect("output path");
    let comments = vec![("X0".into(), "start".into()), ("Y0".into(), "lamp".into())];
    let bytes = opendvp::dvp_file::write(&[0x0024, 0x00c5, 0x0f00], "Test", &comments).unwrap();
    std::fs::write(path, bytes).unwrap();
}
