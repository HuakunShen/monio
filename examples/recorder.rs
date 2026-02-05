//! Event recorder example - record and playback macros.
//!
//! Usage:
//!   cargo run --example recorder --features recorder -- record macro.json
//!   cargo run --example recorder --features recorder -- playback macro.json
//!   cargo run --example recorder --features recorder -- playback-fast macro.json

use std::env;
use std::time::Duration;

#[cfg(feature = "recorder")]
use monio::{EventRecorder, Recording};

fn main() -> monio::Result<()> {
    #[cfg(not(feature = "recorder"))]
    {
        eprintln!("This example requires the 'recorder' feature.");
        eprintln!("Run with: cargo run --example recorder --features recorder -- ...");
        std::process::exit(1);
    }

    #[cfg(feature = "recorder")]
    {
        let args: Vec<String> = env::args().collect();

        if args.len() < 3 {
            println!("Usage:");
            println!("  {} record <filename>   - Record for 5 seconds", args[0]);
            println!(
                "  {} playback <filename>  - Playback with original timing",
                args[0]
            );
            println!(
                "  {} playback-fast <filename> - Playback as fast as possible",
                args[0]
            );
            return Ok(());
        }

        let command = &args[1];
        let filename = &args[2];

        match command.as_str() {
            "record" => {
                println!("Recording for 5 seconds...");
                println!("Perform some keyboard and mouse actions!");

                let recording = EventRecorder::record_for(Duration::from_secs(5))?;

                println!("\nRecording complete!");
                println!("Total events: {}", recording.event_count());
                println!("Duration: {:?}", recording.duration());

                recording.save(filename)?;
                println!("Saved to: {}", filename);
            }
            "playback" => {
                let recording = Recording::load(filename)?;
                println!("Playing back {} events...", recording.event_count());
                println!("Press Ctrl+C to stop");

                recording.playback()?;
                println!("Playback complete!");
            }
            "playback-fast" => {
                let recording = Recording::load(filename)?;
                println!(
                    "Playing back {} events (fast mode)...",
                    recording.event_count()
                );

                recording.playback_fast()?;
                println!("Playback complete!");
            }
            _ => {
                eprintln!("Unknown command: {}", command);
            }
        }
    }

    Ok(())
}
