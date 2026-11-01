//! `il_words PROGRAM.il` prints the ES2 program words OpenDVP encodes (hex, one line, END included).
fn main() -> Result<(), String> {
    let path = std::env::args().nth(1).ok_or("kullanım: il_words PROGRAM.il")?;
    let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let program = opendvp::plc::compile_il(&text).map_err(|e| format!("{e:?}"))?;
    let words = opendvp::es2_encoder::encode(&program)?.words;
    println!(
        "{}",
        words.iter().map(|w| format!("{w:04X}")).collect::<Vec<_>>().join(" ")
    );
    Ok(())
}
