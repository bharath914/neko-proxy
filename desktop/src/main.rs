//! The Linux desktop app. A native window that talks to `nekod` over the
//! client API and nothing else.

mod app;
mod edit;
mod automation;
mod backend;
mod fmt;
mod model;
mod motion;
mod paint;
mod platform;
mod style;
mod theme;
mod ui;
#[cfg(test)]
mod visual;

use backend::{Backend, CoreOptions, DemoBackend, RealBackend};
use std::path::PathBuf;

struct Args {
    demo: bool,
    live: bool,
    socket: Option<PathBuf>,
    nekod: Option<PathBuf>,
    listen: String,
    help: bool,
}

fn parse_args() -> Args {
    let mut a = Args {
        demo: false,
        live: false,
        socket: None,
        nekod: None,
        listen: "127.0.0.1:8899".into(),
        help: false,
    };
    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--demo" => a.demo = true,
            "--live" => a.live = true,
            "--socket" => a.socket = it.next().map(PathBuf::from),
            "--nekod" => a.nekod = it.next().map(PathBuf::from),
            "--listen" => {
                if let Some(v) = it.next() {
                    a.listen = v;
                }
            }
            "-h" | "--help" => a.help = true,
            _ => {}
        }
    }
    a
}

fn main() -> eframe::Result {
    // CLI verbs are implemented by the core executable, including all parsing.
    if std::env::args().nth(1).as_deref() == Some("import-har") {
        let status = platform::find_nekod().and_then(|path| {
            std::process::Command::new(path).args(std::env::args_os().skip(1)).status().ok()
        });
        if status.is_none() { eprintln!("Could not launch nekod; build or install neko-cli first"); }
        std::process::exit(status.and_then(|s| s.code()).unwrap_or(1));
    }
    let args = parse_args();
    if args.help {
        println!(
            "proxy: desktop app for the proxy core\n\n\
             Options:\n  --socket PATH   API socket (default: $XDG_RUNTIME_DIR/neko/api.sock)\n  \
             --nekod PATH   core binary to start if none is running\n  \
             --listen ADDR   proxy address for a core we start (default 127.0.0.1:8899)\n  \
             --demo          run on sample data, without a core\n  \
             --live          with --demo, keep adding flows"
        );
        return Ok(());
    }

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Neko Proxy")
            .with_app_id("dev.nekoproxy.Neko")
            .with_inner_size([1320.0, 820.0])
            .with_min_inner_size([960.0, 560.0]),
        ..Default::default()
    };

    eframe::run_native(
        "Neko",
        options,
        Box::new(move |cc| {
            style::install_fonts(&cc.egui_ctx);
            let backend: Box<dyn Backend> = if args.demo {
                Box::new(DemoBackend::new(240, args.live))
            } else {
                let socket = args.socket.clone().unwrap_or_else(platform::default_socket);
                let nekod = args.nekod.clone().or_else(platform::find_nekod);
                Box::new(RealBackend::start(
                    CoreOptions {
                        socket,
                        nekod,
                        listen: args.listen.clone(),
                    },
                    cc.egui_ctx.clone(),
                ))
            };
            let sys = platform::SystemAppearance::watch(cc.egui_ctx.clone());
            let prefs = platform::Prefs::load();
            Ok(Box::new(app::App::new(&cc.egui_ctx, backend, sys, prefs)))
        }),
    )
}
