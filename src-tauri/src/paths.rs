//! Where the launcher keeps things: %APPDATA%\.mistinn on Windows, ~/.mistinn elsewhere.

use std::path::PathBuf;

pub fn root() -> PathBuf {
    let base = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from("."));
    base.join(".mistinn")
}

pub fn libraries() -> PathBuf {
    root().join("libraries")
}

pub fn assets() -> PathBuf {
    root().join("assets")
}

pub fn versions() -> PathBuf {
    root().join("versions")
}

pub fn runtimes() -> PathBuf {
    root().join("runtimes")
}

pub fn instance(id: &str) -> PathBuf {
    root().join("instances").join(id)
}

pub fn config_file() -> PathBuf {
    root().join("launcher.json")
}

pub fn accounts_file() -> PathBuf {
    root().join("accounts.dat")
}

/// Path of a Maven artifact ("group:name:version[:classifier]") under the libraries folder.
pub fn maven_path(coordinate: &str) -> Option<PathBuf> {
    let (coordinate, extension) = match coordinate.split_once('@') {
        Some((c, ext)) => (c, ext),
        None => (coordinate, "jar"),
    };
    let parts: Vec<&str> = coordinate.split(':').collect();
    if parts.len() < 3 {
        return None;
    }
    let group = parts[0].replace('.', "/");
    let name = parts[1];
    let version = parts[2];
    let file = match parts.get(3) {
        Some(classifier) => format!("{name}-{version}-{classifier}.{extension}"),
        None => format!("{name}-{version}.{extension}"),
    };
    Some(PathBuf::from(group).join(name).join(version).join(file))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maven_paths() {
        assert_eq!(
            maven_path("net.fabricmc:fabric-loader:0.20.1").unwrap(),
            PathBuf::from("net/fabricmc/fabric-loader/0.20.1/fabric-loader-0.20.1.jar")
        );
        assert_eq!(
            maven_path("org.lwjgl:lwjgl:3.4.3:natives-windows").unwrap(),
            PathBuf::from("org/lwjgl/lwjgl/3.4.3/lwjgl-3.4.3-natives-windows.jar")
        );
    }
}
