//! Conservative matching. No dates, preview, quantization or publisher stripping.
use super::types::{MetadataObservation, MetricScope};
use rusqlite::Connection;
use std::collections::{HashMap, HashSet};
#[derive(Clone, Debug)]
pub struct Match {
    pub canonical: Option<i64>,
    pub deployment: Option<i64>,
    pub status: &'static str,
}
type ScopedModelKey = (String, String);
type DeploymentIdentity = (i64, Option<i64>);
#[derive(Default)]
pub struct MatchIndex {
    deployments: HashMap<ScopedModelKey, Vec<DeploymentIdentity>>,
    canonical: HashMap<String, HashSet<i64>>,
    aliases: HashMap<(String, String), HashSet<i64>>,
    decisions: HashMap<(String, String), HashSet<Option<i64>>>,
}
impl MatchIndex {
    pub fn load(conn: &Connection) -> rusqlite::Result<Self> {
        let mut index = Self::default();
        let mut stmt =
            conn.prepare("SELECT id,platform,model_id,canonical_model_id FROM models")?;
        for row in stmt.query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, Option<i64>>(3)?,
            ))
        })? {
            let (id, p, m, c) = row?;
            index
                .deployments
                .entry((p.to_lowercase(), m.to_lowercase()))
                .or_default()
                .push((id, c));
        }
        let mut stmt = conn.prepare("SELECT id,canonical_id FROM canonical_models")?;
        for row in stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))? {
            let (id, key) = row?;
            index
                .canonical
                .entry(key.to_lowercase())
                .or_default()
                .insert(id);
        }
        let mut stmt = conn.prepare(
            "SELECT source,source_model_id,canonical_model_id FROM model_aliases WHERE verified=1",
        )?;
        for row in stmt.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, i64>(2)?,
            ))
        })? {
            let (s, k, c) = row?;
            index
                .aliases
                .entry((s, k.to_lowercase()))
                .or_default()
                .insert(c);
        }
        let mut stmt = conn.prepare(
            "SELECT source,source_model_id,canonical_model_id FROM metadata_match_decisions",
        )?;
        for row in stmt.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, Option<i64>>(2)?,
            ))
        })? {
            let (s, k, c) = row?;
            index
                .decisions
                .entry((s, k.to_lowercase()))
                .or_default()
                .insert(c);
        }
        Ok(index)
    }
    pub fn resolve(&self, o: &MetadataObservation) -> Match {
        self.resolve_identity(
            &o.source,
            &o.source_model_id,
            &o.model_id,
            o.provider.as_deref(),
            o.scope,
        )
    }
    pub fn resolve_identity(
        &self,
        source: &str,
        source_model_id: &str,
        model_id: &str,
        provider: Option<&str>,
        scope: MetricScope,
    ) -> Match {
        let key = (source.to_owned(), source_model_id.to_lowercase());
        let decisions = self.decisions.get(&key);
        if decisions.is_some_and(|s| s.len() > 1)
            || (decisions.is_none() && self.aliases.get(&key).is_some_and(|s| s.len() > 1))
        {
            return Match {
                canonical: None,
                deployment: None,
                status: "ambiguous",
            };
        }
        let explicit = decisions.and_then(|s| s.iter().next().copied());
        if explicit == Some(None) {
            return Match {
                canonical: None,
                deployment: None,
                status: "rejected",
            };
        }
        let alias = explicit.flatten().or_else(|| {
            self.aliases
                .get(&key)
                .and_then(|s| s.iter().next().copied())
        });
        if scope != MetricScope::CanonicalModel {
            if let Some(p) = provider {
                // Reviewed provider spelling aliases only; never strip gateway prefixes.
                let p = match p {
                    "google-ai-studio" => "google",
                    "github-models" => "github",
                    other => other,
                };
                if let Some(deployments) = self
                    .deployments
                    .get(&(p.to_lowercase(), model_id.to_lowercase()))
                {
                    if deployments.len() != 1 {
                        return Match {
                            canonical: None,
                            deployment: None,
                            status: "ambiguous",
                        };
                    }
                    let (id, c) = deployments[0];
                    return Match {
                        canonical: alias.or(c),
                        deployment: Some(id),
                        status: "matched",
                    };
                }
            }
            // A canonical hint cannot make a provider deployment eligible.
            return Match {
                canonical: alias,
                deployment: None,
                status: if alias.is_some() {
                    "canonicalOnly"
                } else {
                    "unmatched"
                },
            };
        }
        if let Some(c) = alias {
            return Match {
                canonical: Some(c),
                deployment: None,
                status: "matched",
            };
        }
        let mut candidates = HashSet::new();
        for k in [model_id, source_model_id] {
            if let Some(ids) = self.canonical.get(&k.to_lowercase()) {
                candidates.extend(ids);
            }
        }
        let status = if candidates.len() == 1 {
            "matched"
        } else if candidates.len() > 1 {
            "ambiguous"
        } else {
            "unmatched"
        };
        Match {
            canonical: if candidates.len() == 1 {
                candidates.into_iter().next()
            } else {
                None
            },
            deployment: None,
            status,
        }
    }
}
