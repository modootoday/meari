use meari_native::infrastructure::window::{WindowRendererChoice, run_resize_latency_probe};
fn main() {
    let choice = match std::env::args().nth(1).as_deref() {
        Some("cpu") | None => WindowRendererChoice::Cpu,
        Some("gpu") => WindowRendererChoice::Gpu,
        Some("gpu-software") => WindowRendererChoice::GpuAllowSoftware,
        _ => {
            eprintln!("usage: meari-resize-probe [cpu|gpu|gpu-software]");
            std::process::exit(2);
        }
    };
    if let Err(error) = run_resize_latency_probe(choice) {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
