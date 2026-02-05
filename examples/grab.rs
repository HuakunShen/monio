//! Event grabbing example - block specific keys from reaching other apps.
//!
//! Run with: cargo run --example grab
//!
//! This example demonstrates the grab feature which allows you to:
//! - Intercept keyboard and mouse events
//! - Block specific events from reaching other applications
//! - Create global hotkeys
//!
//! IMPORTANT: This will actually block keys! Press Ctrl+C to exit.
//!
//! In this example, the Q, W, and E keys are blocked. Try typing them -
//! they won't work in other applications while this example is running!
//!
//! Platform support:
//! - macOS: Full support (CGEventTap)
//! - Windows: Full support (low-level hooks)
//! - Linux/X11: Falls back to listen mode (XRecord cannot grab)
//! - Linux/Wayland: Requires evdev feature + input group permissions

use monio::{Event, EventType, Key, grab};
use std::sync::atomic::{AtomicU32, Ordering};
use std::thread;
use std::time::Duration;

static BLOCKED_COUNT: AtomicU32 = AtomicU32::new(0);

fn get_display_server() -> &'static str {
    if std::env::var("WAYLAND_DISPLAY").is_ok() {
        "Wayland"
    } else if std::env::var("DISPLAY").is_ok() {
        "X11"
    } else {
        "unknown"
    }
}

fn check_linux_permissions() {
    #[cfg(target_os = "linux")]
    {
        use std::process::Command;
        
        // Check if we're on Wayland
        let is_wayland = std::env::var("WAYLAND_DISPLAY").is_ok();
        if is_wayland {
            eprintln!("\n⚠️  WAYLAND DETECTED - IMPORTANT LIMITATION:");
            eprintln!("   Wayland compositors use libinput which may not recognize");
            eprintln!("   re-injected events from virtual devices. Grab mode may");
            eprintln!("   block all input instead of passing through allowed events.");
            eprintln!("   This is a fundamental limitation of evdev+ uinput on Wayland.");
            eprintln!();
        }
        
        // Check system groups (what you'll have after re-login)
        let output = Command::new("id")
            .arg("-Gn")
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok());
        
        let has_input_group = output.as_ref().map_or(false, |g| g.contains("input"));
        
        // Check current process groups (active now)
        let current_groups = Command::new("groups")
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok());
        
        let currently_has_input = current_groups.as_ref().map_or(false, |g| g.contains("input"));
        
        if has_input_group && !currently_has_input {
            eprintln!("⚠️  You are in the 'input' group, but the change hasn't taken effect yet.");
            eprintln!("   Please log out and log back in, or run: newgrp input");
            eprintln!();
        } else if !has_input_group {
            eprintln!("⚠️  WARNING: You are not in the 'input' group.");
            eprintln!("   The grab feature requires access to /dev/input devices.");
            eprintln!("   Run: sudo usermod -aG input $USER");
            eprintln!("   Then log out and back in.\n");
        }

        // Check if /dev/input is accessible
        let input_accessible = std::fs::read_dir("/dev/input").is_ok();
        if !input_accessible {
            eprintln!("⚠️  /dev/input is not accessible.");
            if !has_input_group {
                eprintln!("   This should be fixed after adding yourself to the 'input' group.");
            } else {
                eprintln!("   You may need to create a udev rule:");
                eprintln!("   echo 'SUBSYSTEM==\"input\", GROUP=\"input\", MODE=\"660\"' | sudo tee /etc/udev/rules.d/99-input.rules");
            }
            eprintln!();
        }

        // Check uinput access
        use std::os::unix::fs::MetadataExt;
        
        let uinput_metadata = std::fs::metadata("/dev/uinput").ok();
        let (uinput_uid, uinput_mode) = uinput_metadata.as_ref().map_or((0, 0), |m| {
            (m.uid(), m.mode() & 0o777)
        });
        
        // uinput is often root:root 0600 - need udev rule to change it
        let uinput_needs_rule = uinput_uid == 0 && uinput_mode == 0o600;
        
        if uinput_needs_rule {
            eprintln!("⚠️  CRITICAL: /dev/uinput is root-only (mode 0600).");
            eprintln!("   Grab mode requires uinput access to re-inject events.");
            eprintln!("   Create this udev rule (copy & paste the entire block):");
            eprintln!();
            eprintln!("   echo 'KERNEL==\"uinput\", GROUP=\"input\", MODE=\"0660\"' | sudo tee /etc/udev/rules.d/99-uinput.rules");
            eprintln!("   sudo udevadm control --reload-rules");
            eprintln!("   sudo udevadm trigger --subsystem-match=input");
            eprintln!();
            eprintln!("   Then log out and back in (or run: sudo chmod 660 /dev/uinput for immediate test)");
            eprintln!();
        }
        
        // Quick fix attempt: try newgrp
        if has_input_group && !currently_has_input {
            eprintln!("💡 Quick fix: Try running this example with 'newgrp input':");
            eprintln!("   newgrp input -c 'cargo run --example grab --features evdev --no-default-features'");
            eprintln!();
        }
    }
}

fn main() {
    println!("monio grab example");
    println!("===================\n");
    println!("Display server: {}\n", get_display_server());
    
    check_linux_permissions();
    
    // Safety timeout: auto-exit after 10 seconds
    thread::spawn(move || {
        thread::sleep(Duration::from_secs(10));
        println!("\n[10 second timeout reached - exiting]");
        std::process::exit(0);
    });
    println!("Auto-exit in 10 seconds (safety timeout)\n");
    
    println!("This example blocks the following:");
    println!("  - Q key (completely blocked)");
    println!("  - W key (completely blocked)");
    println!("  - E key (completely blocked)");
    println!("\nTry typing q, w, or e - they won't work in other apps!");
    println!("Press Ctrl+C to exit.\n");

    if let Err(e) = grab(|event: &Event| {
        match event.event_type {
            EventType::KeyPressed => {
                if let Some(kb) = &event.keyboard {
                    match kb.key {
                        Key::KeyQ => {
                            let count = BLOCKED_COUNT.fetch_add(1, Ordering::SeqCst) + 1;
                            println!("BLOCKED Q key! (total blocked: {})", count);
                            return None; // Consume - don't pass to other apps
                        }
                        Key::KeyW => {
                            let count = BLOCKED_COUNT.fetch_add(1, Ordering::SeqCst) + 1;
                            println!("BLOCKED W key! (total blocked: {})", count);
                            return None; // Consume
                        }
                        Key::KeyE => {
                            let count = BLOCKED_COUNT.fetch_add(1, Ordering::SeqCst) + 1;
                            println!("BLOCKED E key! (total blocked: {})", count);
                            return None; // Consume
                        }
                        Key::Escape => {
                            println!("Escape pressed - passing through");
                            // Return Some to pass through
                        }
                        _ => {
                            // Other keys pass through silently
                        }
                    }
                }
            }
            EventType::MousePressed => {
                if let Some(mouse) = &event.mouse {
                    // All mouse buttons pass through (not blocked)
                    println!(
                        "Mouse {:?} pressed at ({:.0}, {:.0}) - passing through",
                        mouse.button, mouse.x, mouse.y
                    );
                }
            }
            _ => {}
        }

        // Return Some(event) to pass the event through to other apps
        Some(event.clone())
    }) {
        eprintln!("Error: {}", e);
    }
}
