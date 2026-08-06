use anyhow::Result;

const CURRENT: &str = env!("CARGO_PKG_VERSION");
const RELEASES_API: &str = "https://api.github.com/repos/bharath03-a/GitCortex/releases/latest";

pub fn run() -> Result<()> {
    eprintln!("gcx update\n");
    eprintln!("  current version:  {CURRENT}");

    match fetch_latest_version() {
        Some(latest) => {
            eprintln!("  latest version:   {latest}");
            if latest == CURRENT {
                eprintln!("  you are up to date.\n");
            } else {
                eprintln!("  update available!\n");
            }
        }
        None => {
            eprintln!("  latest version:   (could not check — no curl found)\n");
        }
    }

    let method = detect_install_method();
    eprintln!("  To update ({method}):");
    eprintln!("    {}", update_command(&method));
    Ok(())
}

fn fetch_latest_version() -> Option<String> {
    let output = std::process::Command::new("curl")
        .args([
            "-s",
            "--max-time",
            "5",
            "-H",
            "Accept: application/vnd.github+json",
            "-H",
            "X-GitHub-Api-Version: 2022-11-28",
            RELEASES_API,
        ])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let body = String::from_utf8(output.stdout).ok()?;
    // Parse "tag_name": "v0.2.3" without pulling in serde (already available but keep it simple)
    let tag = body
        .split("\"tag_name\"")
        .nth(1)?
        .split('"')
        .nth(1)?
        .trim_start_matches('v')
        .to_owned();

    if tag.is_empty() {
        None
    } else {
        Some(tag)
    }
}

#[derive(Debug)]
enum InstallMethod {
    Homebrew,
    Cargo,
    Npm,
    Pipx,
    Uv,
    Pip,
    Curl,
}

impl std::fmt::Display for InstallMethod {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InstallMethod::Homebrew => write!(f, "Homebrew"),
            InstallMethod::Cargo => write!(f, "cargo"),
            InstallMethod::Npm => write!(f, "npm"),
            InstallMethod::Pipx => write!(f, "pipx"),
            InstallMethod::Uv => write!(f, "uv tool"),
            InstallMethod::Pip => write!(f, "pip"),
            InstallMethod::Curl => write!(f, "curl installer"),
        }
    }
}

fn detect_install_method() -> InstallMethod {
    let exe = std::env::current_exe()
        .ok()
        .and_then(|p| p.to_str().map(str::to_owned))
        .unwrap_or_default()
        .replace('\\', "/");

    detect_install_method_from_path(&exe)
}

fn detect_install_method_from_path(exe: &str) -> InstallMethod {
    if exe.contains("/Cellar/gitcortex/") || exe.contains("/homebrew/Cellar/gitcortex/") {
        InstallMethod::Homebrew
    } else if exe.contains(".cargo/bin") {
        InstallMethod::Cargo
    } else if exe.contains("node_modules") || exe.contains("npm") {
        InstallMethod::Npm
    } else if exe.contains("/pipx/venvs/") {
        InstallMethod::Pipx
    } else if exe.contains("/uv/tools/") || exe.contains("/uv/tool/") {
        InstallMethod::Uv
    } else if exe.contains("site-packages") || exe.contains("Scripts") {
        InstallMethod::Pip
    } else {
        InstallMethod::Curl
    }
}

fn update_command(method: &InstallMethod) -> &'static str {
    match method {
        InstallMethod::Homebrew => "brew upgrade gitcortex",
        InstallMethod::Cargo => "cargo install gitcortex",
        InstallMethod::Npm => "npm install -g gitcortex@latest",
        InstallMethod::Pipx => "pipx upgrade gitcortex",
        InstallMethod::Uv => "uv tool upgrade gitcortex",
        InstallMethod::Pip => "pip install --upgrade gitcortex",
        InstallMethod::Curl =>
            "curl --proto '=https' --tlsv1.2 -LsSf \\\n      https://github.com/bharath03-a/GitCortex/releases/latest/download/gitcortex-installer.sh | sh",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_package_manager_locations() {
        assert!(matches!(
            detect_install_method_from_path("/opt/homebrew/Cellar/gitcortex/0.7/bin/gcx"),
            InstallMethod::Homebrew
        ));
        assert!(matches!(
            detect_install_method_from_path("/home/me/.local/pipx/venvs/gitcortex/bin/gcx"),
            InstallMethod::Pipx
        ));
        assert!(matches!(
            detect_install_method_from_path("/home/me/.local/share/uv/tools/gitcortex/bin/gcx"),
            InstallMethod::Uv
        ));
    }
}
