//! odyn's overrides of nebula, for seams in crates that hold no TUI types.
//! Each seam in upstream code carries an `// odyn:` comment; `git grep
//! 'odyn:'` lists them.

/// The command name as a string literal, for `concat!` in `const` strings
/// (`concat!("run ", cli_name!(), " rename")`). Use `CLI_NAME` everywhere
/// else. nebula-core re-exports both.
#[macro_export]
macro_rules! cli_name {
    () => {
        "odyn"
    };
}

/// The binary's name: what users type, what agents run from `PATH`, and
/// what `ps` shows. Must match `[[bin]] name` in `crates/nebula/Cargo.toml`.
pub const CLI_NAME: &str = cli_name!();

/// The fork's GitHub repository: where its releases and install script live.
pub const REPO_URL: &str = "https://github.com/eljamo/odyn";

/// The install script that `upgrade`, `ssh` and `tunnel` run (on the remote
/// for the last two), unless `NEBULA_INSTALL_URL` names another.
pub const INSTALL_URL: &str = "https://raw.githubusercontent.com/eljamo/odyn/main/install.sh";

/// GitHub's "latest release" page, for the FOOTER's update indicator.
pub const LATEST_URL: &str = "https://github.com/eljamo/odyn/releases/latest";
