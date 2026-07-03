use rvemu_core::platform::StdioPlatform;
use rvemu_core::Platform;

fn main() {
    let mut args = std::env::args().skip(1);
    let Some(image) = args.next() else {
        eprintln!("usage: rvemu <image.elf-or-bin> [options]");
        std::process::exit(2);
    };
    let mut platform = StdioPlatform::new();
    // Emulator core is not implemented yet (harness comes first, per the
    // project charter). This stub only proves the workspace and the
    // Platform trait wiring compile end to end.
    for b in format!("rvemu: no core yet; would load {image}\n").bytes() {
        platform.console_write(b);
    }
    std::process::exit(1);
}
