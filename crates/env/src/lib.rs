use std::path::Path;
use std::process::Command;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
pub struct EnvReport {
    pub git: bool,
    pub node: Option<String>,
    pub pnpm: Option<String>,
    pub npm: Option<String>,
    pub python: Option<String>,
    pub rust: Option<String>,
    pub go: Option<String>,
    pub docker: bool,
    pub docker_compose: bool,
    pub databases: Vec<String>,
}

fn version_of(cmd: &str, args: &[&str]) -> Option<String> {
    let out = Command::new(cmd).args(args).output().ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&out.stdout);
    Some(s.trim().lines().next()?.trim().to_string())
}

fn exists(cmd: &str) -> bool {
    Command::new(cmd)
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

pub fn detect(root: &Path) -> EnvReport {
    let mut r = EnvReport {
        git: exists("git"),
        ..Default::default()
    };
    r.node = version_of("node", &["--version"]);
    r.pnpm = version_of("pnpm", &["--version"]);
    r.npm = version_of("npm", &["--version"]);
    r.python =
        version_of("python3", &["--version"]).or_else(|| version_of("python", &["--version"]));
    r.rust = version_of("rustc", &["--version"]);
    r.go = version_of("go", &["version"]);
    r.docker = exists("docker");
    r.docker_compose =
        exists("docker-compose") || version_of("docker", &["compose", "version"]).is_some();

    for db in ["psql", "mysql", "mongod", "redis-cli"] {
        if exists(db) {
            r.databases.push(db.to_string());
        }
    }
    let _ = root;
    r
}

/// Well-known required env var names referenced by the project (values never read).
pub fn required_env_vars(root: &Path) -> Vec<String> {
    let mut vars: Vec<String> = Vec::new();
    let candidates = [".env.example", ".env.sample", ".env.template"];
    for name in candidates {
        let p = root.join(name);
        if let Ok(s) = std::fs::read_to_string(&p) {
            for line in s.lines() {
                let line = line.trim();
                if line.is_empty() || line.starts_with('#') {
                    continue;
                }
                if let Some((k, _)) = line.split_once('=') {
                    let k = k.trim();
                    if !k.is_empty() && !vars.contains(&k.to_string()) {
                        vars.push(k.to_string());
                    }
                }
            }
        }
    }
    vars.sort();
    vars
}
