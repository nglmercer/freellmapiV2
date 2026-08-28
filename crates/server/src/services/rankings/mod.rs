//! Ranking enrichment and model-ID matching.

pub mod enrich;
pub mod match_ids;
pub mod routing;
pub mod sources;
pub mod types;

pub use enrich::{enrich_rankings, enrich_rankings_if_stale, reset_rankings, EnrichResult};
pub use match_ids::{build_rank_index, identity, ModelIdentity, RankLookup};
pub use types::{
    ExternalBenchmark, RankingError, RankingSource, RankingSourceProvider, SourceBenchmark,
};
