//! Event statistics example - collect and display input statistics.
//!
//! Usage:
//!   cargo run --example statistics --features statistics --
//!
//! Press Ctrl+C to stop and see the final statistics.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

#[cfg(feature = "statistics")]
use monio::StatisticsCollector;

fn main() -> monio::Result<()> {
    #[cfg(not(feature = "statistics"))]
    {
        eprintln!("This example requires the 'statistics' feature.");
        eprintln!("Run with: cargo run --example statistics --features statistics");
        std::process::exit(1);
    }

    #[cfg(feature = "statistics")]
    {
        let running = Arc::new(AtomicBool::new(true));
        let r = running.clone();

        // Handle Ctrl+C
        ctrlc::set_handler(move || {
            r.store(false, Ordering::SeqCst);
            println!("\nStopping...");
        })
        .expect("Error setting Ctrl-C handler");

        println!("Collecting input statistics...");
        println!("Type, click, and move your mouse!");
        println!("Press Ctrl+C to stop and see results.\n");

        let mut collector = StatisticsCollector::new();
        collector.start()?;

        // Print intermediate stats every 5 seconds
        while running.load(Ordering::SeqCst) {
            std::thread::sleep(Duration::from_secs(5));

            if !running.load(Ordering::SeqCst) {
                break;
            }

            let stats = collector.snapshot();
            println!("--- Progress ---");
            println!(
                "Events: {} | Keys: {} | Clicks: {} | Moves: {}",
                stats.total_events(),
                stats.key_press_count,
                stats.mouse_click_count,
                stats.mouse_move_count
            );

            if stats.needs_break(Duration::from_secs(30)) {
                println!("⚠️  You've been typing for 30+ seconds. Consider taking a break!");
            }
        }

        let stats = collector.stop()?;
        println!("\n{}", stats.summary());
    }

    Ok(())
}
