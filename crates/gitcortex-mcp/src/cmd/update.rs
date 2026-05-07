use anyhow::Result;

const CURRENT: &str = env!("CARGO_PKG_VERSION");
const RELEASES_API: &str =
    "https://api.github.com/repos/bharath03-a/GitCortex/releases/latest";

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
            "--max-time", "5",
            "-H", "Accept: application/vnd.github+json",
            "-H", "X-GitHub-Api-Version: 2022-11-28",
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
        .nth(2)?
        .trim_start_matches('v')
        .to_owned();

    if tag.is_empty() { None } else { Some(tag) }
}

#[derive(Debug)]
enum InstallMethod {
    Cargo,
    Npm,
    Pip,
    Curl,
}

impl std::fmt::Display for InstallMethod {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InstallMethod::Cargo => write!(f, "cargo"),
            InstallMethod::Npm   => write!(f, "npm"),
            InstallMethod::Pip   => write!(f, "pip/pipx/uv"),
            InstallMethod::Curl  => write!(f, "curl installer"),
        }
    }
}

fn detect_install_method() -> InstallMethod {
    let exe = std::env::current_exe()
        .ok()
        .and_then(|p| p.to_str().map(str::to_owned))
        .unwrap_or_default()
        .replace('\\', "/");

    if exe.contains(".cargo/bin") {
        InstallMethod::Cargo
    } else if exe.contains("node_modules") || exe.contains("npm") {
        InstallMethod::Npm
    } else if exe.contains("site-packages") || exe.contains("Scripts") || exe.contains("pipx") {
        InstallMethod::Pip
    } else {
        InstallMethod::Curl
    }
}

fn update_command(method: &InstallMethod) -> &'static str {
    match method {
        InstallMethod::Cargo => "cargo install gitcortex",
        InstallMethod::Npm   => "npm install -g gitcortex@latest",
        InstallMethod::Pip   => "pip install --upgrade gitcortex",
        InstallMethod::Curl  =>
            "curl --proto '=https' --tlsv1.2 -LsSf \\\n      https://github.com/bharath03-a/GitCortex/releases/latest/download/gcx-installer.sh | sh",
    }
}
