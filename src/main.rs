use bevy::app::{App, Plugin};
use bevy::utils::*;
use bevy::window::*;
use bevy::prelude::*;
use bevy_micro_tools::programs::draw::DrawPlugin;
use clap::Parser;

const WINDOW_SIZE: UVec2 = UVec2::new(1920, 1080);

fn main() {
    let program = Program::from_cli_args();
    App::new()
        .add_plugins(DefaultPlugins
            .set(WindowPlugin {
                primary_window: Some(Window {
                    title: format!("{:?}", program),
                    position: WindowPosition::Centered(MonitorSelection::Primary),
                    resolution: WindowResolution::new(WINDOW_SIZE.x as f32, WINDOW_SIZE.y as f32)
                        .with_scale_factor_override(1.0),
                    ..default() 
                }), ..default()
            }))
        .add_plugins(program)
        .run();
}

#[derive(Debug, Parser)]
struct CliArgs {
    #[arg(short, long)]
    program: String,
}

#[derive(Debug, Copy, Clone)]
pub enum Program {
    Draw,
}

impl Program {
    fn from_cli_args() -> Self {
        let args = CliArgs::parse();
        match args.program.as_str() {
            "draw" => Self::Draw,
            _ => panic!("Unknown program: {}, supplied args: {:?}", args.program, args),
        }
    }
}

impl Plugin for Program {
    fn build(&self, app: &mut App) {
        match self {
            Self::Draw => app.add_plugins(DrawPlugin),
        };
    }
}