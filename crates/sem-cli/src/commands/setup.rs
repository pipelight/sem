use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::Command;

use colored::Colorize;

fn wrapper_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    PathBuf::from(home).join(".local/bin/sem-diff-wrapper")
}

fn wrapper_script() -> String {
    format!(
        "#!/bin/sh\n\
         # Wrapper for git diff.external: translates git's 7-arg format to sem diff\n\
         # Args: path old-file old-hex old-mode new-file new-hex new-mode\n\
         exec sem diff \"$2\" \"$5\"\n"
    )
}

pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    let path = wrapper_path();
    let dir = path.parent().unwrap();

    // Create ~/.local/bin/ if needed
    if !dir.exists() {
        fs::create_dir_all(dir)?;
        println!(
            "{} Created {}",
            "✓".green().bold(),
            dir.display()
        );
    }

    // Write wrapper script
    fs::write(&path, wrapper_script())?;
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755))?;
    println!(
        "{} Created wrapper script at {}",
        "✓".green().bold(),
        path.display()
    );

    // Set diff.external globally
    let status = Command::new("git")
        .args(["config", "--global", "diff.external", "sem-diff-wrapper"])
        .status()?;
    if !status.success() {
        return Err("Failed to set diff.external in git config".into());
    }
    println!(
        "{} Set git config --global diff.external = sem-diff-wrapper",
        "✓".green().bold(),
    );

    println!(
        "\n{} Running `git diff` in any repo will now use sem.",
        "Done!".green().bold()
    );
    println!("To revert, run: sem unsetup");

    Ok(())
}

pub fn unsetup() -> Result<(), Box<dyn std::error::Error>> {
    // Unset diff.external
    let status = Command::new("git")
        .args(["config", "--global", "--unset", "diff.external"])
        .status()?;
    if status.success() {
        println!(
            "{} Removed diff.external from global git config",
            "✓".green().bold(),
        );
    } else {
        println!(
            "{} diff.external was not set in global git config",
            "✓".green().bold(),
        );
    }

    // Remove wrapper script
    let path = wrapper_path();
    if path.exists() {
        fs::remove_file(&path)?;
        println!(
            "{} Removed wrapper script at {}",
            "✓".green().bold(),
            path.display()
        );
    }

    println!(
        "\n{} git diff restored to default behavior.",
        "Done!".green().bold()
    );

    Ok(())
}
