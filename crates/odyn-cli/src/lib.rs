//! odyn's overrides of the nebula command line. Each seam in upstream code
//! carries an `// odyn:` comment.

use clap::{Arg, Command};
use odyn::CLI_NAME;

/// The name upstream's help text uses.
const UPSTREAM: &str = "nebula";

/// Returns `cmd` with its help text, and every subcommand's, naming odyn:
/// the descriptions (`about`, `long_about`), each argument's help, and the
/// `Examples:` blocks. Upstream's text stays as it is in `cli.rs`, so
/// upstream changes to it still merge.
pub fn with_odyn_help(cmd: Command) -> Command {
    let mut cmd = cmd;
    if let Some(text) = cmd.get_about().map(|t| rename(&t.to_string())) {
        cmd = cmd.about(text);
    }
    if let Some(text) = cmd.get_long_about().map(|t| rename(&t.to_string())) {
        cmd = cmd.long_about(text);
    }
    if let Some(text) = cmd.get_after_help().map(|t| rename(&t.to_string())) {
        cmd = cmd.after_help(text);
    }
    cmd = cmd.mut_args(rename_arg);
    let names: Vec<String> = cmd
        .get_subcommands()
        .map(|sub| sub.get_name().to_string())
        .collect();
    names
        .iter()
        .fold(cmd, |cmd, name| cmd.mut_subcommand(name, with_odyn_help))
}

fn rename_arg(arg: Arg) -> Arg {
    let mut arg = arg;
    if let Some(text) = arg.get_help().map(|t| rename(&t.to_string())) {
        arg = arg.help(text);
    }
    if let Some(text) = arg.get_long_help().map(|t| rename(&t.to_string())) {
        arg = arg.long_help(text);
    }
    arg
}

/// Renames "nebula" (and "Nebula") where it is a whole word: the command, or
/// the product in prose. A name inside a file name or a path stays, because
/// that is the real name on disk: `.nebula.json`, `nebula-settings.json`,
/// `~/dotfiles/nebula`. `NEBULA_*` variables stay too.
///
/// Example blocks line up their descriptions in one column. Renames before a
/// line's first run of two or more spaces shift that column, so the line is
/// padded (or trimmed) there to keep it in place.
pub fn rename(text: &str) -> String {
    text.split('\n')
        .map(rename_line)
        .collect::<Vec<_>>()
        .join("\n")
}

fn rename_line(line: &str) -> String {
    let indent = line.len() - line.trim_start().len();
    let gap = line[indent..].find("  ").map(|at| indent + at);
    let (head, tail) = line.split_at(gap.unwrap_or(line.len()));
    let (head, renamed) = rename_words(head);
    let (tail, _) = rename_words(tail);
    if gap.is_none() || renamed == 0 {
        return head + &tail;
    }
    let shift = (UPSTREAM.len() as isize - CLI_NAME.len() as isize) * renamed as isize;
    let tail = if shift >= 0 {
        " ".repeat(shift as usize) + &tail
    } else {
        // Keep at least two spaces between the command and its description.
        let run = tail.len() - tail.trim_start().len();
        let remove = (-shift as usize).min(run.saturating_sub(2));
        tail[remove..].to_string()
    };
    head + &tail
}

/// Renames each whole-word "nebula" in `text`, returning the result and how
/// many it renamed.
fn rename_words(text: &str) -> (String, usize) {
    let mut out = String::with_capacity(text.len());
    let mut count = 0;
    let mut rest = text;
    // Both spellings, without lowercasing: that can move byte offsets.
    let next_at = |s: &str| match (s.find(UPSTREAM), s.find("Nebula")) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (a, b) => a.or(b),
    };
    while let Some(at) = next_at(rest) {
        let (before, from) = rest.split_at(at);
        let word = &from[..UPSTREAM.len()];
        let after = &from[UPSTREAM.len()..];
        out.push_str(before);
        let prev = out.chars().last();
        let next = after.chars().next();
        let next2 = after.chars().nth(1);
        let whole = !prev.is_some_and(|c| c.is_alphanumeric() || "._-/".contains(c))
            && !next.is_some_and(|c| c.is_alphanumeric() || "_-/".contains(c))
            && !(next == Some('.') && next2.is_some_and(char::is_alphanumeric));
        if whole {
            if word.starts_with('N') {
                let mut chars = CLI_NAME.chars();
                out.extend(chars.next().map(|c| c.to_ascii_uppercase()));
                out.push_str(chars.as_str());
            } else {
                out.push_str(CLI_NAME);
            }
            count += 1;
        } else {
            out.push_str(word);
        }
        rest = after;
    }
    out.push_str(rest);
    (out, count)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `cmd` with `description` starting at `column`.
    fn padded(cmd: &str, description: &str, column: usize) -> String {
        let line = format!("  {cmd}");
        format!("{line}{}{description}", " ".repeat(column - line.len()))
    }

    #[test]
    fn renames_the_command_and_keeps_the_description_column() {
        let input = format!(
            "Examples:\n{}\n{}",
            padded("nebula add .", "add the repo you are standing in", 35),
            padded(
                "make install && nebula reload",
                "cut over to a local build",
                35
            ),
        );
        let out = rename(&input);
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(
            lines[1],
            padded(
                &format!("{CLI_NAME} add ."),
                "add the repo you are standing in",
                35
            )
        );
        assert_eq!(
            lines[2],
            padded(
                &format!("make install && {CLI_NAME} reload"),
                "cut over to a local build",
                35
            )
        );
    }

    #[test]
    fn a_rename_in_the_description_does_not_move_the_column() {
        let input = padded("nebula ssh user@server", "open the remote's nebula", 35);
        let expected = padded(
            &format!("{CLI_NAME} ssh user@server"),
            &format!("open the remote's {CLI_NAME}"),
            35,
        );
        assert_eq!(rename(&input), expected);
    }

    #[test]
    fn renames_prose_backticks_and_a_capitalised_name() {
        let out = rename("Nebula keeps a tree. A bare `nebula` opens the TUI; nebula's daemon.");
        let mut title = CLI_NAME.chars();
        let title: String = title
            .next()
            .map(|c| c.to_ascii_uppercase())
            .into_iter()
            .chain(title)
            .collect();
        assert_eq!(
            out,
            format!(
                "{title} keeps a tree. A bare `{CLI_NAME}` opens the TUI; {CLI_NAME}'s daemon."
            )
        );
    }

    #[test]
    fn leaves_file_names_paths_and_variables() {
        let text = "write nebula-settings.json, read .nebula.json, \
                    ~/dotfiles/nebula and NEBULA_LOG, call nebula_core";
        assert_eq!(rename(text), text);
    }

    #[test]
    fn leaves_a_continuation_line_alone() {
        let text = "                                      branch from a named start point";
        assert_eq!(rename(text), text);
    }
}
