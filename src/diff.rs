use colored::Colorize;
use similar::{ChangeTag, TextDiff};
use std::io::{self, Write};
use std::path::Path;

pub struct DiffViewer;

impl DiffViewer {
    /// Render a colorized unified diff between old_text and new_text
    pub fn render_diff(file_path: &Path, old_text: &str, new_text: &str) -> String {
        let diff = TextDiff::from_lines(old_text, new_text);
        let mut output = String::new();

        let header = format!(
            "╭── 📝 DIFF PREVIEW: {} ──────────────────────────────╮",
            file_path.display()
        );
        output.push_str(&header.cyan().bold().to_string());
        output.push('\n');

        let mut has_changes = false;
        for (idx, group) in diff.grouped_ops(3).iter().enumerate() {
            if idx > 0 {
                output.push_str(&format!("{}\n", "--- @@ ---".blue()));
            }
            for op in group {
                for change in diff.iter_changes(op) {
                    has_changes = true;
                    let sign = match change.tag() {
                        ChangeTag::Delete => "-".red().bold(),
                        ChangeTag::Insert => "+".green().bold(),
                        ChangeTag::Equal => " ".normal(),
                    };
                    let line_str = change.value();
                    let line_clean = line_str.trim_end_matches(['\r', '\n']);
                    let formatted = match change.tag() {
                        ChangeTag::Delete => format!("{} {}", sign, line_clean.red()),
                        ChangeTag::Insert => format!("{} {}", sign, line_clean.green()),
                        ChangeTag::Equal => format!("{} {}", sign, line_clean.dimmed()),
                    };
                    output.push_str(&formatted);
                    output.push('\n');
                }
            }
        }

        if !has_changes {
            output.push_str(&format!("  {}\n", "(Tidak ada perubahan isi file)".dimmed()));
        }

        output.push_str(&"╰─────────────────────────────────────────────────────────────╯".cyan().bold().to_string());
        output.push('\n');

        output
    }

    /// Prompt the human user in the loop with the color diff preview
    pub fn prompt_diff_confirmation(file_path: &Path, old_text: &str, new_text: &str, auto_confirm: bool) -> bool {
        if auto_confirm {
            return true;
        }

        if old_text == new_text {
            return true;
        }

        let diff_str = Self::render_diff(file_path, old_text, new_text);
        print!("{}", diff_str);

        print!("{} [Y/n]: ", "⚡ Terapkan perubahan berkas ini?".bold().yellow());
        let _ = io::stdout().flush();

        let mut input = String::new();
        if io::stdin().read_line(&mut input).is_ok() {
            let trimmed = input.trim().to_lowercase();
            trimmed.is_empty() || trimmed == "y" || trimmed == "yes"
        } else {
            false
        }
    }
}
