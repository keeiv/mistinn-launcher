//! Offline skins. An offline account has no skin server, so the game gives it one of its default
//! skins, chosen by the account's UUID, with wide or slim arms by the same choice. The launcher
//! keeps the player's own skin, in a wide-armed and a slim-armed version, and at launch writes a
//! small resource pack that puts it in place of every default skin. Whichever one the game picks,
//! it shows the player's skin with the right arms. This works in any game version (the textures
//! are written at both the paths of 1.19.3 and later and the older ones) and needs no mod.
//! Only offline accounts get the pack: a Microsoft account's skin comes from Mojang.

use crate::auth::Account;
use crate::paths;
use anyhow::{bail, Result};
use std::path::{Path, PathBuf};

/// The pack's folder name under the instance's resourcepacks, and its id in options.txt.
pub const PACK: &str = "mistinn-skin";
/// The resource pack the offline skin is served from, as the game's options name it.
const PACK_ID: &str = "file/mistinn-skin";

/// The default skins since 1.19.3, each at `textures/entity/player/{wide,slim}/<name>.png`.
const DEFAULTS: [&str; 9] = ["steve", "alex", "ari", "efe", "kai", "makena", "noor", "sunny", "zuri"];

/// An offline skin, kept in both arm widths.
pub struct Skin {
    pub wide: Vec<u8>,
    pub slim: Vec<u8>,
    /// The arms the player drew it for; the other version is converted from it.
    pub slim_arms: bool,
}

/// Where offline skins are kept.
fn dir() -> PathBuf {
    paths::root().join("skins")
}

/// An account's skin files: the wide-armed image, the slim-armed one and the marker for slim arms.
fn files(uuid: &str) -> (PathBuf, PathBuf, PathBuf) {
    let d = dir();
    (
        d.join(format!("{uuid}-wide.png")),
        d.join(format!("{uuid}-slim.png")),
        d.join(format!("{uuid}-slim-arms")),
    )
}

/// A PNG image of exactly 64 by 64 pixels (the IHDR chunk follows the 8-byte signature).
pub fn is_skin_png(bytes: &[u8]) -> bool {
    const SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    if bytes.len() < 24 || bytes[..8] != SIGNATURE || &bytes[12..16] != b"IHDR" {
        return false;
    }
    let width = u32::from_be_bytes([bytes[16], bytes[17], bytes[18], bytes[19]]);
    let height = u32::from_be_bytes([bytes[20], bytes[21], bytes[22], bytes[23]]);
    width == 64 && height == 64
}

/// Saves an account's skin after checking both images are 64×64 PNGs.
pub fn save(uuid: &str, skin: &Skin) -> Result<()> {
    if !is_skin_png(&skin.wide) || !is_skin_png(&skin.slim) {
        bail!("a skin is a 64×64 PNG image");
    }
    std::fs::create_dir_all(dir())?;
    let (wide, slim, arms) = files(uuid);
    std::fs::write(wide, &skin.wide)?;
    std::fs::write(slim, &skin.slim)?;
    if skin.slim_arms {
        std::fs::write(arms, b"")?;
    } else if arms.exists() {
        std::fs::remove_file(arms)?;
    }
    Ok(())
}

/// An account's saved skin, if it has one.
pub fn load(uuid: &str) -> Option<Skin> {
    let (wide, slim, arms) = files(uuid);
    Some(Skin {
        wide: std::fs::read(wide).ok()?,
        slim: std::fs::read(slim).ok()?,
        slim_arms: arms.exists(),
    })
}

/// Removes an account's skin: back to the default.
pub fn remove(uuid: &str) -> Result<()> {
    let (wide, slim, arms) = files(uuid);
    for file in [wide, slim, arms] {
        if file.exists() {
            std::fs::remove_file(file)?;
        }
    }
    Ok(())
}

/// Before a launch: writes the skin pack for an offline account that has a skin, or takes it
/// away for any other, and lists it in options.txt accordingly. Returns whether it is on.
pub fn apply(game_dir: &Path, account: &Account) -> Result<bool> {
    let pack = game_dir.join("resourcepacks").join(PACK);
    let skin = match account {
        Account::Offline { uuid, .. } => load(uuid),
        Account::Microsoft { .. } => None,
    };
    if pack.exists() {
        std::fs::remove_dir_all(&pack)?;
    }
    let on = skin.is_some();
    if let Some(skin) = skin {
        let textures = pack.join("assets/minecraft/textures/entity");
        for (kind, png) in [("wide", &skin.wide), ("slim", &skin.slim)] {
            let folder = textures.join("player").join(kind);
            std::fs::create_dir_all(&folder)?;
            for name in DEFAULTS {
                std::fs::write(folder.join(format!("{name}.png")), png)?;
            }
        }
        // before 1.19.3 there were only these two
        std::fs::write(textures.join("steve.png"), &skin.wide)?;
        std::fs::write(textures.join("alex.png"), &skin.slim)?;
        // old versions read pack_format, new ones min_format and max_format; the wide ranges keep
        // it compatible with every version (it holds nothing but textures)
        let meta = serde_json::json!({
            "pack": {
                "description": "霧宿啟動器：離線皮膚",
                "pack_format": 15,
                "supported_formats": [1, 9999],
                "min_format": 1,
                "max_format": 9999
            }
        });
        std::fs::write(pack.join("pack.mcmeta"), serde_json::to_string_pretty(&meta)?)?;
    }
    enable(&game_dir.join("options.txt"), on)?;
    Ok(on)
}

/// Adds the pack to (or takes it out of) the resource packs options.txt lists, keeping the rest.
/// It is never put on the incompatible list: the game takes a compatible pack off that list
/// and then leaves it unselected for that start (Options.loadSelectedResourcePacks), so a pack
/// listed there would never load. Its wide format range makes it compatible everywhere.
fn enable(options: &Path, on: bool) -> Result<()> {
    let text = std::fs::read_to_string(options).unwrap_or_default();
    let mut lines: Vec<String> = text.lines().map(str::to_string).collect();
    for key in ["resourcePacks", "incompatibleResourcePacks"] {
        let index = lines.iter().position(|l| l.starts_with(&format!("{key}:")));
        let current: Vec<String> = index
            .and_then(|i| serde_json::from_str::<Vec<String>>(&lines[i][key.len() + 1..]).ok())
            .unwrap_or_else(|| {
                if key == "resourcePacks" {
                    vec!["vanilla".to_string()]
                } else {
                    vec![]
                }
            });
        let mut packs: Vec<String> = current.into_iter().filter(|p| p != PACK_ID).collect();
        if on && key == "resourcePacks" {
            // last in the list is on top
            packs.push(PACK_ID.to_string());
        }
        let line = format!("{key}:{}", serde_json::to_string(&packs)?);
        match index {
            Some(i) => lines[i] = line,
            None => lines.push(line),
        }
    }
    if let Some(parent) = options.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(options, lines.join("\n") + "\n")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png(width: u32, height: u32) -> Vec<u8> {
        let mut bytes = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 13];
        bytes.extend_from_slice(b"IHDR");
        bytes.extend_from_slice(&width.to_be_bytes());
        bytes.extend_from_slice(&height.to_be_bytes());
        bytes
    }

    #[test]
    fn only_64_square_pngs_are_skins() {
        assert!(is_skin_png(&png(64, 64)));
        assert!(!is_skin_png(&png(64, 32)));
        assert!(!is_skin_png(&png(128, 128)));
        assert!(!is_skin_png(b"not a png at all, just some bytes"));
    }

    #[test]
    fn options_list_the_pack_once_and_lose_it_again() {
        let dir = std::env::temp_dir().join(format!("mistinn-skin-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let options = dir.join("options.txt");
        std::fs::write(&options, "lang:zh_tw\nresourcePacks:[\"vanilla\",\"fabric\"]\n").unwrap();
        enable(&options, true).unwrap();
        enable(&options, true).unwrap();
        let text = std::fs::read_to_string(&options).unwrap();
        assert!(
            text.contains("resourcePacks:[\"vanilla\",\"fabric\",\"file/mistinn-skin\"]"),
            "{text}"
        );
        assert!(
            text.contains("incompatibleResourcePacks:[]"),
            "never on the incompatible list: {text}"
        );
        assert!(text.contains("lang:zh_tw"));
        enable(&options, false).unwrap();
        let text = std::fs::read_to_string(&options).unwrap();
        assert!(text.contains("resourcePacks:[\"vanilla\",\"fabric\"]"), "{text}");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
