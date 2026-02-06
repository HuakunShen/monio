use monio::{display_at_point, displays, mouse_position};

fn main() -> monio::Result<()> {
    let (x, y) = mouse_position()?;
    println!("Current mouse position: ({:.1}, {:.1})", x, y);

    match display_at_point(x, y)? {
        Some(display) => {
            println!("\nMouse is on display {}:", display.id);
            println!(
                "  Bounds: {:.0},{:.0} - {:.0}x{:.0}",
                display.bounds.x, display.bounds.y, display.bounds.width, display.bounds.height
            );
            println!("  Scale factor: {:.1}x", display.scale_factor);
            println!("  Refresh rate: {:?} Hz", display.refresh_rate);
            println!(
                "  Primary: {}",
                if display.is_primary { "Yes" } else { "No" }
            );

            let rel_x = x - display.bounds.x;
            let rel_y = y - display.bounds.y;
            println!(
                "\nRelative position on display: ({:.1}, {:.1})",
                rel_x, rel_y
            );
        }
        None => {
            println!(
                "\nMouse position ({:.1}, {:.1}) is outside all known displays!",
                x, y
            );
        }
    }

    let all_displays = displays()?;
    println!("\nAll displays ({} total):", all_displays.len());
    for display in all_displays {
        let marker = if display.bounds.contains(x, y) {
            " <-- mouse here"
        } else {
            ""
        };
        println!(
            "  Display {}: {}x{} @ ({:.0}, {:.0}){}",
            display.id,
            display.bounds.width,
            display.bounds.height,
            display.bounds.x,
            display.bounds.y,
            marker
        );
    }

    Ok(())
}
