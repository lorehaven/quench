//! `cargo run -p quench-qr --example dump -- <L|M|Q|H> <text>`: the symbol as text, for comparing with other encoders.
fn main() {
    let mut args = std::env::args().skip(1);
    let level = match args.next().as_deref() {
        Some("L") => quench_qr::EcLevel::L,
        Some("Q") => quench_qr::EcLevel::Q,
        Some("H") => quench_qr::EcLevel::H,
        _ => quench_qr::EcLevel::M,
    };
    let text = args.next().unwrap_or_default();
    match quench_qr::QrCode::with_level(text.as_bytes(), level) {
        Ok(code) => {
            eprintln!("version {} mask {}", code.version(), code.mask());
            print!("{}", code.to_text());
        }
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(2);
        }
    }
}
