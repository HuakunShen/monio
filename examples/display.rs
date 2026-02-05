use monio::{displays, primary_display, system_settings};

fn main() -> monio::Result<()> {
    let primary = primary_display()?;
    println!("Primary display: {primary:?}");

    let all = displays()?;
    println!("Displays ({})", all.len());
    for display in all {
        println!("  {display:?}");
    }

    let settings = system_settings()?;
    println!("System settings: {settings:?}");

    Ok(())
}
