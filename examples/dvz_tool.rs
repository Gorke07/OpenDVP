//! `.dvz` helper. `dvz_tool read FILE.dvz` prints the package and its program as IL;
//! `dvz_tool write MODEL PROGRAM.il OUT.dvz` writes a program-only package (e.g. MODEL=ES2).
fn main() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .as_slice()
    {
        ["read", path] => {
            let data = std::fs::read(path).map_err(|e| e.to_string())?;
            let package = opendvp::dvz::read(&data)?;
            println!(
                "model={:?} (index {:#x}) program={} words D={} C16={} C32={} T={} file_registers={} comment={}B",
                package.model_name(),
                package.model_index,
                package.program.len(),
                package.d.len(),
                package.c16.len(),
                package.c32.len(),
                package.t.len(),
                package.file_registers.len(),
                package.comment.len()
            );
            match opendvp::es2_program_decode::decode(&package.program) {
                Ok(il) => println!("{il}"),
                Err(e) => println!("IL çözülemedi: {e:?}"),
            }
            Ok(())
        }
        ["write", model, source, out] => {
            let index = opendvp::dvz::MODEL_INDICES
                .iter()
                .find(|(name, _)| name.eq_ignore_ascii_case(model))
                .ok_or("Bilinmeyen model")?
                .1;
            let text = std::fs::read_to_string(source).map_err(|e| e.to_string())?;
            let program = opendvp::plc::compile_il(&text).map_err(|e| format!("{e:?}"))?;
            let words = opendvp::es2_encoder::encode(&program)?.words;
            std::fs::write(out, opendvp::dvz::write(index, &words)).map_err(|e| e.to_string())
        }
        _ => {
            Err("Kullanım: dvz_tool read FILE.dvz | dvz_tool write MODEL PROGRAM.il OUT.dvz".into())
        }
    }
}
