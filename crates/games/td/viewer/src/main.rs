//! TD Viewer - A Bevy-based web renderer for tower defense matches.
//!
//! This application connects to the td-mcp-server via a proxy and renders
//! matches in real-time. It's designed to compile to WASM for browser use.

mod game;
mod networking;
mod rendering;
mod ui;

use bevy::prelude::*;

fn main() {
    // Initialize WASM logging
    console_error_panic_hook::set_once();
    tracing_wasm::set_as_global_default();

    let mut app = App::new();

    // Configure window
    app.add_plugins(
        DefaultPlugins
            .set(WindowPlugin {
                primary_window: Some(Window {
                    title: "TD Viewer".to_string(),
                    resolution: (800u32, 700u32).into(),
                    canvas: Some("#bevy-canvas".to_string()),
                    fit_canvas_to_parent: true,
                    prevent_default_event_handling: false,
                    ..default()
                }),
                ..default()
            }),
    );

    // Add game resources
    app.init_resource::<game::GameStateCache>()
        .init_resource::<game::RenderConfig>();

    // Add plugins
    app.add_plugins((
        networking::NetworkingPlugin,
        rendering::RenderingPlugin,
        ui::UiPlugin,
    ));

    // Run
    app.run();
}
