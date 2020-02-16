extern crate gl;
extern crate glutin;
#[macro_use]
extern crate lalrpop_util;
extern crate bytes;
extern crate glm;
extern crate half;
extern crate image;
extern crate notify;
extern crate openexr;
extern crate regex;
extern crate rust_rocket;
extern crate time;
extern crate wavefront_obj;

use std::env;
use std::path::Path;
use std::sync::mpsc::channel;
use std::time::Duration;

use notify::{watcher, DebouncedEvent, RecursiveMode, Watcher};

mod ast;
mod astvisitor;
mod bytecode;
mod color;
mod demoscene;
mod gl_resources;
mod imageio;
mod runtime;
mod sync;
mod types;

lalrpop_mod!(grammar);

use sync::SyncTracker;

fn try_load_demo(path: &Path) -> Option<demoscene::DemoScene> {
    demoscene::DemoScene::from_file(&path)
        .map_err(|e| println!("Error while loading demo:\n{}", e))
        .ok()
}

fn create_sync_tracks(sync_tracker: &mut dyn sync::SyncTracker, scene: &demoscene::DemoScene) {
    scene
        .get_bytecode()
        .get_sync_tracks()
        .iter()
        .for_each(|track| sync_tracker.require_track(track));
}

fn run_demo(filename: &str, size: (u32, u32)) {
    let mut size = glutin::dpi::LogicalSize::new(size.0 as f64, size.1 as f64);
    let mut events_loop = glutin::EventsLoop::new();
    let window = glutin::WindowBuilder::new()
        .with_title("Demoengine")
        .with_dimensions(size);
    let window_context = glutin::ContextBuilder::new()
        .with_vsync(true)
        .with_gl_profile(glutin::GlProfile::Core)
        .build_windowed(window, &events_loop)
        .unwrap();

    let mut dpi_factor = window_context.window().get_hidpi_factor();

    let window_context = unsafe { window_context.make_current().unwrap() };

    unsafe {
        gl::load_with(|symbol| window_context.get_proc_address(symbol) as *const _);
        gl::ClearColor(0.0, 0.0, 0.0, 1.0);
    }

    let path = Path::new(filename);
    let mut demo = try_load_demo(path);
    let mut sync = sync::RocketSyncTracker::new(24.0).expect("Expected a running sync tracker");
    demo.as_ref().map(|demo| create_sync_tracks(&mut sync, demo));

    // Watch the directory for changes
    let (tx, rx) = channel();
    let mut watcher = watcher(tx, Duration::from_millis(100)).unwrap();
    watcher.watch(path.parent().unwrap(), RecursiveMode::Recursive).unwrap();

    let mut running = true;
    while running {
        events_loop.poll_events(|event| match event {
            glutin::Event::WindowEvent { event, .. } => match event {
                glutin::WindowEvent::CloseRequested => running = false,
                glutin::WindowEvent::Resized(logical_size) => {
                    dpi_factor = window_context.window().get_hidpi_factor();
                    window_context.resize(logical_size.to_physical(dpi_factor));
                    size = logical_size;
                }
                _ => (),
            },
            _ => (),
        });

        if let Some(demo) = demo.as_mut() {
            sync.update();
            let time = sync.get_time();

            let physical_size = size.to_physical(dpi_factor);
            if let Err(err) = demo.draw(
                physical_size.width as f32,
                physical_size.height as f32,
                time as f32,
                &sync,
            ) {
                println!("Error while rendering scene: \n{}", err);
            }
        }

        window_context.swap_buffers().unwrap();
        std::thread::sleep(std::time::Duration::from_millis(16));

        // Look if any files have changed
        let mut recreate_scene = false;
        for event in rx.try_iter() {
            if let DebouncedEvent::Write(_) = event {
                recreate_scene = true;
            }
        }
        if recreate_scene {
            println!("Reloading...");
            demo.take();
            demo = try_load_demo(&path);
            demo.as_ref().map(|demo| create_sync_tracks(&mut sync, demo));
        }
    }
}

fn main() {
    if env::args().len() != 2 {
        println!("Usage: ./demoengine SCRIPT");
        return;
    }
    let filename = env::args().skip(1).next().unwrap();
    let initial_size = (1024, 768);

    run_demo(&filename, initial_size);
}
