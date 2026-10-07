mod appearance;
mod assets;
mod menus;
mod ui;
mod worker;
use anyhow::{Context, Result};
use gpui_kit::{AppContext, Bounds, TitlebarOptions, WindowBounds, WindowOptions, point, px, size};
fn main() -> Result<()> {
    let mut demo = false;
    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "--demo" => demo = true,
            "--version" => {
                println!("ocal {}", env!("CARGO_PKG_VERSION"));
                return Ok(());
            }
            "--help" | "-h" => {
                println!(
                    "Ocal — native calendar for Omarchy\n\nUsage: ocal [--demo]\n\n--demo  Isolated sample calendar; never connects to Google\n\nArrows move the time cursor · Shift+arrows extend 15-minute ranges\nEnter opens the highlighted event or adds at the cursor/range\nc / Ctrl+N new event · t today · 1/2/3/4 day/week/month/agenda\nCtrl+Enter save · Escape dismiss · / search · ? all keys"
                );
                return Ok(());
            }
            _ => anyhow::bail!("Unknown argument {arg}. Use --help"),
        }
    }
    let data = std::env::var_os("XDG_DATA_HOME")
        .map(std::path::PathBuf::from)
        .unwrap_or(
            std::env::home_dir()
                .context("Cannot find your home directory")?
                .join(".local/share"),
        )
        .join("ocal/calendar.sqlite3");
    gpui_kit::application()
        .with_assets(assets::Assets)
        .run(move |cx| {
            gpui_omarchy::init(cx);
            ui::bind_keys(cx);
            let worker = worker::Worker::start(data, demo);
            cx.open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                        point(px(80.), px(80.)),
                        size(px(1440.), px(940.)),
                    ))),
                    titlebar: Some(TitlebarOptions {
                        title: Some("Ocal".into()),
                        ..Default::default()
                    }),
                    window_min_size: Some(size(px(480.), px(480.))),
                    app_id: Some("ocal".into()),
                    ..Default::default()
                },
                move |window, cx| cx.new(|cx| ui::Ocal::new(worker, demo, window, cx)),
            )
            .expect("Open Ocal window");
            cx.activate(true);
        });
    Ok(())
}
