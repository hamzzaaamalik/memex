//! CLI module for additional command utilities

pub mod commands;

use anyhow::Result;
use colored::*;
use std::io::{self, Write};

use crate::database::models::{MemoryItem, Session};

/// Interactive CLI utilities
pub struct InteractiveCli;

impl InteractiveCli {
    /// Prompt for user confirmation
    pub fn confirm(message: &str, default: bool) -> Result<bool> {
        let default_char = if default { "Y/n" } else { "y/N" };
        print!("{} ({}): ", message, default_char);
        io::stdout().flush()?;

        let mut input = String::new();
        io::stdin().read_line(&mut input)?;

        let input = input.trim().to_lowercase();

        match input.as_str() {
            "y" | "yes" => Ok(true),
            "n" | "no" => Ok(false),
            "" => Ok(default),
            _ => {
                println!("{}", "Please enter 'y' or 'n'".yellow());
                Self::confirm(message, default)
            }
        }
    }

    /// Prompt for text input with optional default
    pub fn prompt_text(message: &str, default: Option<&str>) -> Result<String> {
        if let Some(def) = default {
            print!("{} [{}]: ", message, def);
        } else {
            print!("{}: ", message);
        }
        io::stdout().flush()?;

        let mut input = String::new();
        io::stdin().read_line(&mut input)?;

        let input = input.trim();
        if input.is_empty() {
            Ok(default.unwrap_or("").to_string())
        } else {
            Ok(input.to_string())
        }
    }

    /// Prompt for number input
    pub fn prompt_number<T>(message: &str, default: Option<T>) -> Result<T>
    where
        T: std::str::FromStr + std::fmt::Display + Copy,
        T::Err: std::fmt::Debug,
    {
        let default_text = default.map(|d| d.to_string());
        let input = Self::prompt_text(message, default_text.as_deref())?;

        if input.is_empty() && default.is_some() {
            Ok(default.unwrap())
        } else {
            input
                .parse()
                .map_err(|_| anyhow::anyhow!("Invalid number format"))
        }
    }

    /// Select from a list of options
    pub fn select_from_list<T: std::fmt::Display>(message: &str, options: &[T]) -> Result<usize> {
        println!("{}", message.bold());
        for (i, option) in options.iter().enumerate() {
            println!("  {}. {}", i + 1, option);
        }

        let selection: usize = Self::prompt_number("Select option", None::<usize>)?;

        if selection == 0 || selection > options.len() {
            return Err(anyhow::anyhow!("Invalid selection"));
        }

        Ok(selection - 1)
    }

    /// Show progress bar for long operations
    pub fn show_progress(current: usize, total: usize, message: &str) {
        let percentage = if total > 0 {
            (current as f32 / total as f32 * 100.0) as usize
        } else {
            0
        };

        let bar_length = 50;
        let filled_length = (percentage * bar_length) / 100;
        let bar = "█".repeat(filled_length) + &"░".repeat(bar_length - filled_length);

        print!(
            "\r{} [{}] {}% ({}/{})",
            message,
            bar.blue(),
            percentage,
            current,
            total
        );
        io::stdout().flush().ok();

        if current >= total {
            println!(); // New line when complete
        }
    }

    /// Display a table of data
    pub fn display_table<T: TableRow>(title: &str, headers: &[&str], rows: &[T]) {
        println!("\n{}", title.green().bold());

        // Calculate column widths
        let mut col_widths = headers.iter().map(|h| h.len()).collect::<Vec<_>>();

        for row in rows {
            let row_data = row.to_row();
            for (i, cell) in row_data.iter().enumerate() {
                if i < col_widths.len() {
                    col_widths[i] = col_widths[i].max(cell.len());
                }
            }
        }

        // Print header
        print!("┌");
        for (i, width) in col_widths.iter().enumerate() {
            print!("{}", "─".repeat(width + 2));
            if i < col_widths.len() - 1 {
                print!("┬");
            }
        }
        println!("┐");

        print!("│");
        for (i, (header, width)) in headers.iter().zip(&col_widths).enumerate() {
            print!(" {:<width$} ", header.bold(), width = width);
            if i < headers.len() - 1 {
                print!("│");
            }
        }
        println!("│");

        // Print separator
        print!("├");
        for (i, width) in col_widths.iter().enumerate() {
            print!("{}", "─".repeat(width + 2));
            if i < col_widths.len() - 1 {
                print!("┼");
            }
        }
        println!("┤");

        // Print rows
        for row in rows {
            let row_data = row.to_row();
            print!("│");
            for (i, (cell, width)) in row_data.iter().zip(&col_widths).enumerate() {
                print!(" {:<width$} ", cell, width = width);
                if i < row_data.len() - 1 {
                    print!("│");
                }
            }
            println!("│");
        }

        // Print footer
        print!("└");
        for (i, width) in col_widths.iter().enumerate() {
            print!("{}", "─".repeat(width + 2));
            if i < col_widths.len() - 1 {
                print!("┴");
            }
        }
        println!("┘");
    }
}

/// Trait for types that can be displayed as table rows
pub trait TableRow {
    fn to_row(&self) -> Vec<String>;
}

/// Implement TableRow for common types
impl TableRow for MemoryItem {
    fn to_row(&self) -> Vec<String> {
        vec![
            self.id[..8].to_string(), // Truncated ID
            self.user_id.clone(),
            if self.content.len() > 30 {
                format!("{}...", &self.content[..30])
            } else {
                self.content.clone()
            },
            format!("{:.1}", self.importance),
            self.created_at.format("%Y-%m-%d").to_string(),
        ]
    }
}

impl TableRow for Session {
    fn to_row(&self) -> Vec<String> {
        vec![
            self.id[..8].to_string(), // Truncated ID
            self.name
                .as_ref()
                .unwrap_or(&"(unnamed)".to_string())
                .clone(),
            self.memory_count.to_string(),
            self.last_active.format("%Y-%m-%d %H:%M").to_string(),
        ]
    }
}

/// Format file sizes in human-readable format
pub fn format_bytes(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];
    let mut size = bytes as f64;
    let mut unit_index = 0;

    while size >= 1024.0 && unit_index < UNITS.len() - 1 {
        size /= 1024.0;
        unit_index += 1;
    }

    if unit_index == 0 {
        format!("{} {}", size as u64, UNITS[unit_index])
    } else {
        format!("{:.1} {}", size, UNITS[unit_index])
    }
}

/// Format duration in human-readable format
pub fn format_duration(seconds: i64) -> String {
    if seconds < 60 {
        format!("{}s", seconds)
    } else if seconds < 3600 {
        format!("{}m {}s", seconds / 60, seconds % 60)
    } else if seconds < 86400 {
        let hours = seconds / 3600;
        let minutes = (seconds % 3600) / 60;
        format!("{}h {}m", hours, minutes)
    } else {
        let days = seconds / 86400;
        let hours = (seconds % 86400) / 3600;
        format!("{}d {}h", days, hours)
    }
}

/// Colorize text based on value ranges
pub fn colorize_importance(importance: f32) -> colored::ColoredString {
    match importance {
        i if i >= 0.8 => format!("{:.1}", i).bright_green(),
        i if i >= 0.5 => format!("{:.1}", i).yellow(),
        i if i >= 0.2 => format!("{:.1}", i).blue(),
        i => format!("{:.1}", i).dimmed(),
    }
}

/// Colorize memory counts
pub fn colorize_count(count: usize, high_threshold: usize) -> colored::ColoredString {
    if count >= high_threshold {
        count.to_string().bright_green()
    } else if count >= high_threshold / 2 {
        count.to_string().yellow()
    } else {
        count.to_string().dimmed()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_bytes() {
        assert_eq!(format_bytes(512), "512 B");
        assert_eq!(format_bytes(1024), "1.0 KB");
        assert_eq!(format_bytes(1536), "1.5 KB");
        assert_eq!(format_bytes(1024 * 1024), "1.0 MB");
        assert_eq!(format_bytes(1024 * 1024 * 1024), "1.0 GB");
    }

    #[test]
    fn test_format_duration() {
        assert_eq!(format_duration(30), "30s");
        assert_eq!(format_duration(90), "1m 30s");
        assert_eq!(format_duration(3661), "1h 1m");
        assert_eq!(format_duration(90061), "1d 1h");
    }
}
