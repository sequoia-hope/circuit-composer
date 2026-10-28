//! The research library: a database of individual claims, not documents. Each claim is one
//! sentence with its scope, the attributes it affects, its citations, a status, and links that
//! support, refine or contradict other claims. Contradictions are kept, both ways round.
//!
//! The database is `library/claims/*.json` (claims about a circuit class, sources in
//! `library/sources.json`) together with every package's `rules.json` (claims about one chip,
//! sources in the package's `sources/sources.json`). A citation finds its source in its own set
//! first, then in the library's; `rp2350/rp2350_hdg` names a package's source from anywhere, so a
//! document is listed once.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const STATUSES: [&str; 5] = ["asserted", "contested", "simulated", "promoted", "refuted"];
pub const LINKS: [&str; 3] = ["supports", "refines", "contradicts"];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Source {
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// Where to find a source with no stable URL.
    #[serde(default, rename = "where", skip_serializing_if = "Option::is_none")]
    pub where_: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Citation {
    pub source: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub locator: Option<String>,
    /// A short passage from the source, so a reader can find and check it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quote: Option<String>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claim {
    pub id: String,
    pub statement: String,
    #[serde(default)]
    pub scope: BTreeMap<String, Value>,
    #[serde(default)]
    pub attributes: Vec<String>,
    /// Packages cite one source; library claims often several.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub citation: Option<Citation>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub citations: Vec<Citation>,
    pub status: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub links: Vec<BTreeMap<String, String>>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub enforced_by: Vec<String>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

impl Claim {
    pub fn cites(&self) -> impl Iterator<Item = &Citation> {
        self.citation.iter().chain(&self.citations)
    }

    /// Each link as (kind, target).
    pub fn link_pairs(&self) -> impl Iterator<Item = (&str, &str)> {
        self.links.iter().flat_map(|l| l.iter().map(|(k, v)| (k.as_str(), v.as_str())))
    }
}

#[derive(Deserialize)]
struct ClaimFile {
    claims: Vec<Claim>,
}

/// One file's claims and the sources they cite first.
#[derive(Debug)]
pub struct ClaimSet {
    /// `library/buck` or `packages/rp2350`.
    pub name: String,
    pub file: PathBuf,
    pub claims: Vec<Claim>,
    /// The set's own sources; empty for library topics, which share `library/sources.json`.
    pub sources: BTreeMap<String, Source>,
}

#[derive(Debug, Default)]
pub struct Library {
    pub sources: BTreeMap<String, Source>,
    pub sets: Vec<ClaimSet>,
}

fn read<T: for<'de> Deserialize<'de>>(p: &Path) -> Result<T, String> {
    let text = std::fs::read_to_string(p).map_err(|e| format!("{}: {e}", p.display()))?;
    serde_json::from_str(&text).map_err(|e| format!("{}: {e}", p.display()))
}

fn json_files(dir: &Path) -> Result<Vec<PathBuf>, String> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .map_err(|e| format!("{}: {e}", dir.display()))?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .collect();
    files.sort();
    Ok(files)
}

impl Library {
    /// Load `ROOT/library` and every `ROOT/packages/*/rules.json`.
    pub fn load(root: &Path) -> Result<Library, String> {
        let lib = root.join("library");
        let mut out = Library { sources: read(&lib.join("sources.json"))?, sets: Vec::new() };
        for f in json_files(&lib.join("claims"))? {
            let cf: ClaimFile = read(&f)?;
            let name = format!("library/{}", f.file_stem().unwrap().to_string_lossy());
            out.sets.push(ClaimSet { name, file: f, claims: cf.claims, sources: BTreeMap::new() });
        }
        let pkgs = root.join("packages");
        if pkgs.is_dir() {
            let mut dirs: Vec<PathBuf> = std::fs::read_dir(&pkgs).map_err(|e| e.to_string())?.filter_map(|e| e.ok().map(|e| e.path())).filter(|p| p.join("rules.json").is_file()).collect();
            dirs.sort();
            for d in dirs {
                let cf: ClaimFile = read(&d.join("rules.json"))?;
                let src = d.join("sources/sources.json");
                let sources = if src.is_file() { read(&src)? } else { BTreeMap::new() };
                let name = format!("packages/{}", d.file_name().unwrap().to_string_lossy());
                out.sets.push(ClaimSet { name, file: d.join("rules.json"), claims: cf.claims, sources });
            }
        }
        Ok(out)
    }

    pub fn claims(&self) -> impl Iterator<Item = (&ClaimSet, &Claim)> {
        self.sets.iter().flat_map(|s| s.claims.iter().map(move |c| (s, c)))
    }

    pub fn claim(&self, id: &str) -> Option<(&ClaimSet, &Claim)> {
        self.claims().find(|(_, c)| c.id == id)
    }

    /// A citation's source: `package/id` in that package, otherwise the set's own first, then the
    /// library's.
    pub fn source<'a>(&'a self, set: &'a ClaimSet, id: &str) -> Option<&'a Source> {
        if let Some((pkg, sid)) = id.split_once('/') {
            let name = format!("packages/{pkg}");
            return self.sets.iter().find(|s| s.name == name)?.sources.get(sid);
        }
        set.sources.get(id).or_else(|| self.sources.get(id))
    }

    /// Links that point at `id` from other claims, as (kind, from).
    pub fn incoming(&self, id: &str) -> Vec<(String, String)> {
        self.claims().flat_map(|(_, c)| c.link_pairs().filter(|(_, t)| *t == id).map(|(k, _)| (k.to_string(), c.id.clone())).collect::<Vec<_>>()).collect()
    }

    /// The database's integrity: ids, statuses, citations, sources and links.
    pub fn check(&self) -> Report {
        let mut r = Report::default();
        let mut seen: BTreeMap<&str, &str> = BTreeMap::new();
        let mut cited: BTreeSet<String> = BTreeSet::new();
        for (set, c) in self.claims() {
            let at = format!("{} {}", set.name, c.id);
            if let Some(first) = seen.insert(&c.id, &set.name) {
                r.errors.push(format!("{at}: id also used in {first}"));
            }
            if !STATUSES.contains(&c.status.as_str()) {
                r.errors.push(format!("{at}: status {:?} is not one of {STATUSES:?}", c.status));
            }
            if c.statement.trim().is_empty() {
                r.errors.push(format!("{at}: no statement"));
            }
            if c.cites().next().is_none() {
                r.errors.push(format!("{at}: no citation"));
            }
            for cit in c.cites() {
                match self.source(set, &cit.source) {
                    Some(_) => {
                        cited.insert(cit.source.rsplit('/').next().unwrap().to_string());
                    }
                    None => r.errors.push(format!("{at}: cites unknown source {}", cit.source)),
                }
                if cit.locator.as_deref().is_none_or(|l| l.trim().is_empty()) {
                    r.warnings.push(format!("{at}: citation of {} has no page or timestamp", cit.source));
                }
            }
            for (kind, target) in c.link_pairs() {
                if !LINKS.contains(&kind) {
                    r.errors.push(format!("{at}: link kind {kind:?} is not one of {LINKS:?}"));
                }
                let Some((_, t)) = self.claim(target) else {
                    r.errors.push(format!("{at}: {kind} unknown claim {target}"));
                    continue;
                };
                // A dispute is seen from both sides.
                if kind == "contradicts" && !t.link_pairs().any(|(k, b)| k == "contradicts" && b == c.id) {
                    r.errors.push(format!("{at}: contradicts {target}, which does not contradict it back"));
                }
            }
            if c.status == "contested" && !c.link_pairs().any(|(k, _)| k == "contradicts") && !c.extra.contains_key("note") && !c.extra.contains_key("evidence") {
                r.warnings.push(format!("{at}: contested, but names no contradicting claim and has no note"));
            }
            if c.status == "promoted" && c.enforced_by.is_empty() {
                r.errors.push(format!("{at}: promoted, but no tool enforces it (enforced_by is empty)"));
            }
        }
        let mut all_sources: Vec<(String, &str, &Source)> = self.sources.iter().map(|(k, s)| ("library".to_string(), k.as_str(), s)).collect();
        for set in &self.sets {
            all_sources.extend(set.sources.iter().map(|(k, s)| (set.name.clone(), k.as_str(), s)));
        }
        for (owner, id, s) in &all_sources {
            if s.url.is_none() && s.where_.is_none() {
                r.errors.push(format!("{owner} source {id}: neither url nor where"));
            }
            if let Some(h) = &s.sha256 {
                if h.len() != 64 || !h.chars().all(|c| c.is_ascii_hexdigit()) {
                    r.errors.push(format!("{owner} source {id}: sha256 is not 64 hex digits"));
                }
            }
            if !cited.contains(*id) {
                r.warnings.push(format!("{owner} source {id}: no claim cites it"));
            }
        }
        for set in &self.sets {
            for id in set.sources.keys() {
                if self.sources.contains_key(id) {
                    r.errors.push(format!("{} source {id}: shadows the library source of the same id", set.name));
                }
            }
        }
        r.claims = self.claims().count();
        r.sources = all_sources.len();
        for (_, c) in self.claims() {
            *r.by_status.entry(c.status.clone()).or_default() += 1;
        }
        for set in &self.sets {
            r.by_set.insert(set.name.clone(), set.claims.len());
        }
        r.ok = r.errors.is_empty();
        r
    }

    /// Claims matching every filter given.
    pub fn query(&self, q: &Query) -> Vec<Value> {
        let lower = q.text.as_ref().map(|t| t.to_lowercase());
        self.claims()
            .filter(|(set, c)| {
                q.set.as_ref().is_none_or(|s| set.name == *s || set.name.ends_with(&format!("/{s}")))
                    && q.status.as_ref().is_none_or(|s| c.status == *s)
                    && q.attribute.as_ref().is_none_or(|a| c.attributes.contains(a))
                    && q.scope.iter().all(|(k, v)| c.scope.get(k).is_some_and(|x| scope_matches(x, v)))
                    && q.id.as_ref().is_none_or(|p| c.id == *p || c.id.starts_with(&format!("{p}.")))
                    && lower.as_ref().is_none_or(|t| c.statement.to_lowercase().contains(t.as_str()) || c.id.contains(t.as_str()))
            })
            .map(|(set, c)| self.expand(set, c))
            .collect()
    }

    /// A claim with its set, its sources' titles and URLs, and the links that point at it.
    pub fn expand(&self, set: &ClaimSet, c: &Claim) -> Value {
        let mut v = serde_json::to_value(c).unwrap();
        v["set"] = set.name.clone().into();
        let sources: BTreeMap<&str, Value> = c
            .cites()
            .filter_map(|cit| self.source(set, &cit.source).map(|s| (cit.source.as_str(), serde_json::json!({ "title": s.title, "url": s.url, "where": s.where_ }))))
            .collect();
        v["sources"] = serde_json::to_value(sources).unwrap();
        let incoming = self.incoming(&c.id);
        if !incoming.is_empty() {
            v["linked_from"] = incoming.into_iter().map(|(k, from)| serde_json::json!({ k: from })).collect();
        }
        v
    }
}

fn scope_matches(have: &Value, want: &str) -> bool {
    match have {
        Value::String(s) => s.eq_ignore_ascii_case(want),
        Value::Array(a) => a.iter().any(|x| scope_matches(x, want)),
        other => *other == want,
    }
}

#[derive(Debug, Default, Clone)]
pub struct Query {
    /// A set's name (`library/buck`) or its last part (`buck`, `rp2350`).
    pub set: Option<String>,
    pub status: Option<String>,
    pub attribute: Option<String>,
    /// Scope keys and the value each must have (`circuit` = `buck`).
    pub scope: Vec<(String, String)>,
    /// A claim id, or an id prefix ending at a dot (`buck.layout`).
    pub id: Option<String>,
    /// Words in the statement or id.
    pub text: Option<String>,
}

#[derive(Debug, Default, Serialize)]
pub struct Report {
    pub ok: bool,
    pub claims: usize,
    pub sources: usize,
    pub by_status: BTreeMap<String, usize>,
    pub by_set: BTreeMap<String, usize>,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn claim(id: &str, status: &str, links: &[(&str, &str)]) -> Claim {
        serde_json::from_value(serde_json::json!({
            "id": id, "statement": "s", "status": status,
            "citations": [{ "source": "src", "locator": "p.1" }],
            "links": links.iter().map(|(k, v)| serde_json::json!({ *k: v })).collect::<Vec<_>>(),
        }))
        .unwrap()
    }

    fn lib(claims: Vec<Claim>) -> Library {
        let mut sources = BTreeMap::new();
        sources.insert("src".to_string(), Source { title: "t".into(), url: Some("u".into()), where_: None, sha256: None, extra: BTreeMap::new() });
        Library { sources, sets: vec![ClaimSet { name: "library/t".into(), file: PathBuf::new(), claims, sources: BTreeMap::new() }] }
    }

    #[test]
    fn contradictions_go_both_ways() {
        let r = lib(vec![claim("a", "contested", &[("contradicts", "b")]), claim("b", "contested", &[])]).check();
        assert!(!r.ok);
        assert!(r.errors[0].contains("does not contradict it back"), "{:?}", r.errors);
        let r = lib(vec![claim("a", "contested", &[("contradicts", "b")]), claim("b", "contested", &[("contradicts", "a")])]).check();
        assert!(r.ok, "{:?}", r.errors);
    }

    #[test]
    fn unknown_links_sources_and_statuses_fail() {
        let mut c = claim("a", "maybe", &[("supports", "nowhere")]);
        c.citations[0].source = "missing".into();
        let r = lib(vec![c, claim("a", "asserted", &[])]).check();
        let all = r.errors.join("\n");
        for want in ["status \"maybe\"", "unknown claim nowhere", "unknown source missing", "id also used"] {
            assert!(all.contains(want), "{want}: {all}");
        }
    }

    #[test]
    fn package_sources_by_qualified_id() {
        let mut l = lib(vec![claim("a", "asserted", &[])]);
        l.sets[0].claims[0].citations[0].source = "chip/ds".into();
        let mut pkg_sources = BTreeMap::new();
        pkg_sources.insert("ds".to_string(), Source { title: "d".into(), url: Some("u".into()), where_: None, sha256: None, extra: BTreeMap::new() });
        l.sets.push(ClaimSet { name: "packages/chip".into(), file: PathBuf::new(), claims: vec![], sources: pkg_sources });
        let r = l.check();
        assert!(r.ok, "{:?}", r.errors);
        assert!(r.warnings.iter().all(|w| !w.contains("source ds")), "{:?}", r.warnings);
        l.sets[0].claims[0].citations[0].source = "nochip/ds".into();
        assert!(!l.check().ok);
    }

    #[test]
    fn query_by_scope_and_prefix() {
        let mut a = claim("buck.layout.hot_loop", "asserted", &[]);
        a.scope.insert("circuit".into(), "buck".into());
        a.attributes.push("hot_loop_area".into());
        let l = lib(vec![a, claim("crystal.load", "asserted", &[("refines", "buck.layout.hot_loop")])]);
        let q = Query { scope: vec![("circuit".into(), "BUCK".into())], ..Default::default() };
        let hits = l.query(&q);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0]["linked_from"][0]["refines"], "crystal.load");
        assert_eq!(l.query(&Query { id: Some("buck.layout".into()), ..Default::default() }).len(), 1);
        assert_eq!(l.query(&Query { id: Some("buck.lay".into()), ..Default::default() }).len(), 0);
        assert_eq!(l.query(&Query { attribute: Some("hot_loop_area".into()), ..Default::default() }).len(), 1);
    }
}
