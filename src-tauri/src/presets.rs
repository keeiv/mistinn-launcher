//! The three ways to play — 流暢 (smooth), 普通 (normal), 超高 (ultra) — turned into concrete
//! settings for the player's computer: memory for the game, vanilla video options, and whether
//! shaders are worth turning on. These are starting points, not promises of a frame rate.

use crate::hardware::Hardware;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Preset {
    Smooth,
    Normal,
    Ultra,
}

#[derive(Debug, Clone, Serialize)]
pub struct Settings {
    pub preset: Preset,
    pub memory_mb: u64,
    pub render_distance: u32,
    pub simulation_distance: u32,
    pub graphics: &'static str,
    pub particles: u8,
    pub clouds: &'static str,
    pub biome_blend: u32,
    pub mipmaps: u32,
    pub entity_distance: f32,
    pub max_fps: u32,
    pub shaders: bool,
}

pub fn settings(preset: Preset, hw: &Hardware) -> Settings {
    let gpu = hw.gpu_class;
    let pick = |weak: u32, mid: u32, strong: u32| match gpu {
        "strong" => strong,
        "mid" => mid,
        _ => weak,
    };
    // leave the system at least 3 GB, never give the game less than 2 GB
    let share = |fraction: f64, cap: u64| -> u64 {
        let wanted = ((hw.ram_mb as f64 * fraction) as u64).min(cap);
        let ceiling = hw.ram_mb.saturating_sub(3072).max(2048);
        (wanted.max(2048).min(ceiling) / 256) * 256
    };
    // few CPU threads cannot feed long simulation distances
    let sim_cap = if hw.threads <= 4 { 6 } else if hw.threads <= 8 { 10 } else { 12 };
    match preset {
        Preset::Smooth => Settings {
            preset,
            memory_mb: share(0.25, 4096),
            render_distance: pick(6, 8, 10),
            simulation_distance: 5.min(sim_cap),
            graphics: "fast",
            particles: 2,
            clouds: "false",
            biome_blend: 0,
            mipmaps: 0,
            entity_distance: 0.75,
            max_fps: 260,
            shaders: false,
        },
        Preset::Normal => Settings {
            preset,
            memory_mb: share(0.33, 6144),
            render_distance: pick(8, 12, 14),
            simulation_distance: 8.min(sim_cap),
            graphics: "fancy",
            particles: 1,
            clouds: "fast",
            biome_blend: 2,
            mipmaps: 4,
            entity_distance: 1.0,
            max_fps: 260,
            shaders: false,
        },
        Preset::Ultra => Settings {
            preset,
            memory_mb: share(0.4, 8192),
            render_distance: pick(12, 16, 24),
            simulation_distance: 12.min(sim_cap),
            graphics: if gpu == "weak" { "fancy" } else { "fabulous" },
            particles: 0,
            clouds: "true",
            biome_blend: 5,
            mipmaps: 4,
            entity_distance: 1.25,
            max_fps: 260,
            // shaders only where the graphics card can carry them
            shaders: gpu == "strong",
        },
    }
}

/// The player's preset as a starting suggestion: what their machine handles comfortably.
pub fn recommended(hw: &Hardware) -> Preset {
    match (hw.gpu_class, hw.ram_mb) {
        // a "16 GB" machine reports a little less once the system reserves its share
        ("strong", ram) if ram >= 14 * 1024 => Preset::Ultra,
        ("weak", _) => Preset::Smooth,
        (_, ram) if ram < 8 * 1024 => Preset::Smooth,
        _ => Preset::Normal,
    }
}

/// Writes the preset's video options into options.txt, keeping every other line the player set.
pub fn write_options(path: &Path, s: &Settings, first_time: bool) -> std::io::Result<()> {
    let mut lines: BTreeMap<String, String> = BTreeMap::new();
    let existing = std::fs::read_to_string(path).unwrap_or_default();
    for line in existing.lines() {
        if let Some((k, v)) = line.split_once(':') {
            lines.insert(k.to_string(), v.to_string());
        }
    }
    let mut set = |k: &str, v: String| {
        lines.insert(k.to_string(), v);
    };
    set("renderDistance", s.render_distance.to_string());
    set("simulationDistance", s.simulation_distance.to_string());
    set("graphicsPreset", format!("\"{}\"", s.graphics));
    set("particles", s.particles.to_string());
    set("renderClouds", format!("\"{}\"", s.clouds));
    set("biomeBlendRadius", s.biome_blend.to_string());
    set("mipmapLevels", s.mipmaps.to_string());
    set("entityDistanceScaling", s.entity_distance.to_string());
    set("maxFps", s.max_fps.to_string());
    set("enableVsync", "false".into());
    if first_time {
        set("lang", "zh_tw".into());
        set("onboardAccessibility", "false".into());
        set("skipMultiplayerWarning", "true".into());
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let text: String = lines.into_iter().map(|(k, v)| format!("{k}:{v}\n")).collect();
    std::fs::write(path, text)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn machine(ram_gb: u64, vram_gb: u64, threads: usize) -> Hardware {
        Hardware {
            ram_mb: ram_gb * 1024,
            threads,
            cpu: String::new(),
            gpu: String::new(),
            vram_mb: vram_gb * 1024,
            gpu_class: if vram_gb >= 6 { "strong" } else if vram_gb >= 2 { "mid" } else { "weak" },
        }
    }

    #[test]
    fn memory_leaves_room_for_the_system() {
        for (ram, vram) in [(4, 0), (8, 2), (16, 12), (64, 24)] {
            let hw = machine(ram, vram, 8);
            for preset in [Preset::Smooth, Preset::Normal, Preset::Ultra] {
                let s = settings(preset, &hw);
                assert!(s.memory_mb >= 2048 || hw.ram_mb < 5 * 1024, "{ram} GB {preset:?}");
                assert!(s.memory_mb + 3072 <= hw.ram_mb.max(5 * 1024), "{ram} GB {preset:?} gives {}", s.memory_mb);
            }
        }
    }

    #[test]
    fn presets_climb_in_quality() {
        let hw = machine(16, 12, 16);
        let (a, b, c) = (settings(Preset::Smooth, &hw), settings(Preset::Normal, &hw), settings(Preset::Ultra, &hw));
        assert!(a.render_distance < b.render_distance && b.render_distance < c.render_distance);
        assert!(a.memory_mb <= b.memory_mb && b.memory_mb <= c.memory_mb);
        assert!(c.shaders && !b.shaders && !a.shaders);
        assert!(!settings(Preset::Ultra, &machine(16, 1, 8)).shaders, "no shaders on integrated graphics");
    }

    #[test]
    fn a_16_gb_machine_with_a_strong_card_is_offered_ultra() {
        let mut hw = machine(16, 12, 32);
        hw.ram_mb = 15575; // what Windows reports for 16 GB
        assert_eq!(recommended(&hw), Preset::Ultra);
        assert_eq!(recommended(&machine(8, 1, 8)), Preset::Smooth);
    }

    #[test]
    fn options_keep_the_players_own_lines() {
        let dir = std::env::temp_dir().join(format!("mistinn-options-{}", std::process::id()));
        let file = dir.join("options.txt");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(&file, "fov:0.5\nrenderDistance:32\n").unwrap();
        write_options(&file, &settings(Preset::Smooth, &machine(8, 0, 4)), false).unwrap();
        let text = std::fs::read_to_string(&file).unwrap();
        assert!(text.contains("fov:0.5"));
        assert!(text.contains("renderDistance:6"));
        assert!(!text.contains("lang:"));
        std::fs::remove_dir_all(dir).ok();
    }
}
