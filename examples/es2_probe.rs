//! Local protocol investigation utility. Each argument after the port is fn+data hex.
use opendvp::es2_protocol::Client;
fn main() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let port = args
        .first()
        .ok_or("Usage: es2_probe PORT FUNCTION_AND_DATA_HEX ...")?
        .parse::<u16>()
        .map_err(|e| e.to_string())?;
    let mut client = Client::connect(port, 1)?;
    for arg in &args[1..] {
        if let Some(ms) = arg.strip_prefix("wait:") {
            std::thread::sleep(std::time::Duration::from_millis(
                ms.parse::<u64>().map_err(|e| e.to_string())?.min(2000),
            ));
            continue;
        }
        if !arg.len().is_multiple_of(2) || arg.is_empty() {
            return Err("Even-length hex required".into());
        }
        let bytes = (0..arg.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&arg[i..i + 2], 16).map_err(|e| e.to_string()))
            .collect::<Result<Vec<_>, _>>()?;
        match client.request(bytes[0], &bytes[1..]) {
            Ok(data) => println!(
                "{}",
                serde_json::json!({"request":arg,"data":data.iter().map(|b|format!("{b:02X}")).collect::<String>()})
            ),
            Err(e) => return Err(format!("{arg}: {e}")),
        }
    }
    Ok(())
}
