//! Whether a set of Fabric mods will load together. Modrinth lists only the latest build of each
//! mod for a game version; it does not know that, say, a new Sodium refuses the Iris that is out
//! for that version. Each mod says so itself, in the `breaks` and `depends` of its
//! `fabric.mod.json`, and Fabric Loader refuses to start on a conflict. This module reads those
//! declarations and decides which mods to step back to an older build.

use serde_json::Value;
use std::cmp::Ordering;
use std::collections::HashMap;
use std::io::Read;

/// What a mod jar says about itself.
#[derive(Debug, Clone)]
pub struct Manifest {
    pub id: String,
    pub version: String,
    pub depends: Vec<(String, Value)>,
    pub breaks: Vec<(String, Value)>,
}

/// Reads `fabric.mod.json` out of a mod jar.
pub fn manifest(jar: &[u8]) -> Option<Manifest> {
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(jar)).ok()?;
    let mut text = String::new();
    archive.by_name("fabric.mod.json").ok()?.read_to_string(&mut text).ok()?;
    let json: Value = serde_json::from_str(&text).ok()?;
    let map = |key: &str| -> Vec<(String, Value)> {
        json.get(key)
            .and_then(|v| v.as_object())
            .map_or_default(|o| o.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
    };
    Some(Manifest {
        id: json.get("id")?.as_str()?.to_string(),
        version: json.get("version").and_then(|v| v.as_str()).unwrap_or("0").to_string(),
        depends: map("depends"),
        breaks: map("breaks"),
    })
}

/// A conflict between two mods in the set, by their positions in it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Conflict {
    /// `mod` breaks the build of `other` in the set.
    Breaks { r#mod: usize, other: usize },
    /// `mod` needs another build of `other` than the one in the set.
    Needs { r#mod: usize, other: usize },
}

/// Every conflict in the set: who breaks whom, who needs another build of whom. Mods outside the
/// set (Minecraft, the loader, Java, optional extras) are not this check's business.
pub fn conflicts(set: &[Manifest]) -> Vec<Conflict> {
    let by_id: HashMap<&str, usize> = set.iter().enumerate().map(|(i, m)| (m.id.as_str(), i)).collect();
    let mut found = vec![];
    for (i, m) in set.iter().enumerate() {
        for (id, rule) in &m.breaks {
            if let Some(&j) = by_id.get(id.as_str()) {
                if j != i && satisfies(&set[j].version, rule) == Some(true) {
                    found.push(Conflict::Breaks { r#mod: i, other: j });
                }
            }
        }
        for (id, rule) in &m.depends {
            if let Some(&j) = by_id.get(id.as_str()) {
                if j != i && satisfies(&set[j].version, rule) == Some(false) {
                    found.push(Conflict::Needs { r#mod: i, other: j });
                }
            }
        }
    }
    found
}

/// Whether `version` meets a Fabric version rule: a string of terms that must all hold, or a
/// list of such strings of which one must. `None` when the version or the rule cannot be read.
pub fn satisfies(version: &str, rule: &Value) -> Option<bool> {
    match rule {
        Value::String(s) => all_terms(version, s),
        Value::Array(list) => {
            let mut any_unknown = false;
            for item in list {
                match item.as_str().and_then(|s| all_terms(version, s)) {
                    Some(true) => return Some(true),
                    Some(false) => {}
                    None => any_unknown = true,
                }
            }
            if any_unknown {
                None
            } else {
                Some(false)
            }
        }
        _ => None,
    }
}

/// Whether `version` meets every space-separated term (`>=1.0 <2.0`); `None` when a term cannot be
/// read.
fn all_terms(version: &str, terms: &str) -> Option<bool> {
    let mut result = true;
    for term in terms.split_whitespace() {
        result &= term_holds(version, term)?;
    }
    Some(result)
}

/// Whether `version` meets one term such as `>=0.18`, `~1.2` or `*`; `None` when it cannot be read.
fn term_holds(version: &str, term: &str) -> Option<bool> {
    if term == "*" {
        return Some(true);
    }
    let (op, target) = ["<=", ">=", "<", ">", "=", "~", "^"]
        .iter()
        .find_map(|op| term.strip_prefix(op).map(|rest| (*op, rest)))
        .unwrap_or(("=", term));
    let have = Version::parse(version)?;
    if target.split('.').any(|part| part == "x" || part == "X" || part == "*") {
        // a wildcard: 1.9.x is anything from 1.9.0 up to, not including, 1.10.0
        let fixed: Vec<u64> = target
            .split('.')
            .take_while(|p| !matches!(*p, "x" | "X" | "*"))
            .map(|p| p.parse().ok())
            .collect::<Option<_>>()?;
        return Some(have.release.len() >= fixed.len() && have.release[..fixed.len()] == fixed[..]);
    }
    let want = Version::parse(target)?;
    let ord = have.cmp(&want);
    Some(match op {
        "<=" => ord != Ordering::Greater,
        ">=" => ord != Ordering::Less,
        "<" => ord == Ordering::Less,
        ">" => ord == Ordering::Greater,
        "~" => ord != Ordering::Less && have.cmp(&want.bump(1)) == Ordering::Less,
        "^" => ord != Ordering::Less && have.cmp(&want.bump(0)) == Ordering::Less,
        _ => ord == Ordering::Equal,
    })
}

/// A version as Fabric compares it: numbers, then an optional pre-release; build data ignored.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Version {
    release: Vec<u64>,
    pre: Option<Vec<String>>,
}

impl Version {
    fn parse(text: &str) -> Option<Version> {
        let core = text.split('+').next()?;
        let (release, pre) = match core.split_once('-') {
            Some((r, p)) => (r, Some(p.split('.').map(String::from).collect())),
            None => (core, None),
        };
        let release: Vec<u64> = release.split('.').map(|p| p.parse().ok()).collect::<Option<_>>()?;
        if release.is_empty() {
            return None;
        }
        Some(Version { release, pre })
    }

    /// The next version at `place` (0 = major, 1 = minor) with the rest zeroed, without pre-release.
    fn bump(&self, place: usize) -> Version {
        let mut release: Vec<u64> = self.release.iter().copied().take(place + 1).collect();
        while release.len() <= place {
            release.push(0);
        }
        release[place] += 1;
        Version { release, pre: None }
    }
}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> Ordering {
        let len = self.release.len().max(other.release.len());
        for i in 0..len {
            let a = self.release.get(i).copied().unwrap_or(0);
            let b = other.release.get(i).copied().unwrap_or(0);
            match a.cmp(&b) {
                Ordering::Equal => {}
                o => return o,
            }
        }
        match (&self.pre, &other.pre) {
            (None, None) => Ordering::Equal,
            (None, Some(_)) => Ordering::Greater,
            (Some(_), None) => Ordering::Less,
            (Some(a), Some(b)) => {
                for (x, y) in a.iter().zip(b.iter()) {
                    let o = match (x.parse::<u64>(), y.parse::<u64>()) {
                        (Ok(p), Ok(q)) => p.cmp(&q),
                        (Ok(_), Err(_)) => Ordering::Less,
                        (Err(_), Ok(_)) => Ordering::Greater,
                        (Err(_), Err(_)) => x.cmp(y),
                    };
                    if o != Ordering::Equal {
                        return o;
                    }
                }
                a.len().cmp(&b.len())
            }
        }
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn m(id: &str, version: &str, breaks: Value, depends: Value) -> Manifest {
        let pairs = |v: Value| {
            v.as_object()
                .map_or_default(|o| o.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
        };
        Manifest {
            id: id.into(),
            version: version.into(),
            depends: pairs(depends),
            breaks: pairs(breaks),
        }
    }

    #[test]
    fn versions_compare_as_fabric_does() {
        assert_eq!(satisfies("1.11.7+mc26.3", &json!("<=1.12.0")), Some(true));
        assert_eq!(satisfies("1.12.1+mc26.3", &json!("<=1.12.0")), Some(false));
        assert_eq!(satisfies("1.9.1", &json!("<1.9.2")), Some(true));
        assert_eq!(satisfies("1.9.2-beta.1", &json!("<1.9.2")), Some(true));
        assert_eq!(satisfies("0.9.2+mc26.3", &json!(">=0.9.3")), Some(false));
        assert_eq!(satisfies("0.9.3+mc26.3", &json!(">=0.9.3 <0.10")), Some(true));
        assert_eq!(satisfies("1.9.7", &json!("1.9.x")), Some(true));
        assert_eq!(satisfies("1.10.0", &json!("~1.9.0")), Some(false));
        assert_eq!(satisfies("1.9.9", &json!("~1.9.0")), Some(true));
        assert_eq!(satisfies("2.0.0", &json!("^1.2.0")), Some(false));
        assert_eq!(satisfies("0.1", &json!(["<0.0.5", ">=0.1"])), Some(true));
        assert_eq!(satisfies("anything", &json!("*")), Some(true));
        assert_eq!(satisfies("not a version", &json!("<1.0")), None);
    }

    /// The case that stopped the launcher on 2026-11-22: Sodium 0.9.3 came out for 26.3 before
    /// Iris and More Culling had builds it accepts.
    #[test]
    fn a_new_sodium_that_refuses_iris_and_more_culling_is_the_one_in_conflict() {
        let set = [
            m(
                "sodium",
                "0.9.3+mc26.3",
                json!({"iris": "<=1.12.0", "moreculling": "<1.9.2"}),
                json!({}),
            ),
            m("iris", "1.11.7+mc26.3", json!({}), json!({"sodium": "0.9.x"})),
            m("moreculling", "1.9.1", json!({}), json!({})),
        ];
        let found = conflicts(&set);
        assert!(found.contains(&Conflict::Breaks { r#mod: 0, other: 1 }));
        assert!(found.contains(&Conflict::Breaks { r#mod: 0, other: 2 }));
        // with the Sodium before it, all three load
        let older = [m("sodium", "0.9.2+mc26.3", json!({}), json!({})), set[1].clone(), set[2].clone()];
        assert_eq!(conflicts(&older), vec![]);
    }

    #[test]
    fn a_mod_needing_another_build_is_in_conflict() {
        let set = [
            m("sodium-extra", "0.8.0", json!({}), json!({"sodium": ">=0.9.3"})),
            m("sodium", "0.9.2+mc26.3", json!({}), json!({})),
        ];
        assert_eq!(conflicts(&set), vec![Conflict::Needs { r#mod: 0, other: 1 }]);
    }

    #[test]
    fn mods_outside_the_set_are_left_alone() {
        let set = [m(
            "lithium",
            "0.20.0",
            json!({"canary": "*"}),
            json!({"minecraft": "26.3", "fabricloader": ">=0.18"}),
        )];
        assert_eq!(conflicts(&set), vec![]);
    }
}
