// Copyright 2026- SiLeader (Cerussite).
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use std::cmp::Ordering;

/// One item returned by [`EmveDb::search`](crate::EmveDb::search).
#[derive(Debug, Clone, Copy)]
pub struct SearchResultItem {
    /// User-supplied record id.
    pub id: u64,
    /// Metric score used for ordering.
    ///
    /// Scores are always larger-is-better. For L2 search this is the negative
    /// Euclidean distance.
    pub score: f32,
    /// Metric-specific distance value.
    ///
    /// Cosine distance is `1 - cosine_similarity`, L2 distance is Euclidean
    /// distance, and dot-product distance is the dot product.
    pub distance: f32,
}

/// Optional search controls.
///
/// The default value performs an unfiltered exact top-k search over all live
/// records.
#[derive(Default)]
pub struct SearchOptions {
    pub(crate) filter: Option<Box<dyn SearchFilter>>,
    pub(crate) min_score: Option<f32>,
}

impl SearchOptions {
    /// Sets the search filter.
    ///
    /// A filter can reject ids before scoring via [`SearchFilter::filter_id`]
    /// and reject scored results via [`SearchFilter::filter`].
    pub fn with_filter<F>(mut self, filter: F) -> Self
    where
        F: SearchFilter + 'static,
    {
        self.filter = Some(Box::new(filter));
        self
    }

    /// Sets a filter that receives scored result items.
    ///
    /// This is a convenience wrapper for filters that only need the final
    /// [`SearchResultItem`].
    pub fn with_result_filter<F>(self, filter: F) -> Self
    where
        F: Fn(&SearchResultItem) -> bool + Send + Sync + 'static,
    {
        self.with_filter(ResultFilter(filter))
    }

    /// Keeps only results with `score >= min_score`.
    pub fn with_min_score(mut self, min_score: f32) -> Self {
        self.min_score = Some(min_score);
        self
    }
}

/// Predicate hooks used to filter search candidates and results.
pub trait SearchFilter: Send + Sync {
    /// Returns whether a record id should be scored.
    ///
    /// This hook runs before vector scoring and defaults to accepting every id.
    fn filter_id(&self, _id: u64) -> bool {
        true
    }

    /// Returns whether a scored result should be kept.
    ///
    /// This hook runs after scoring and minimum-score filtering, and defaults
    /// to accepting every result.
    fn filter(&self, _result: &SearchResultItem) -> bool {
        true
    }
}

impl<T> SearchFilter for T
where
    T: Fn(u64) -> bool + Send + Sync,
{
    fn filter_id(&self, id: u64) -> bool {
        self(id)
    }
}

struct ResultFilter<F>(F);

impl<F> SearchFilter for ResultFilter<F>
where
    F: Fn(&SearchResultItem) -> bool + Send + Sync,
{
    fn filter(&self, result: &SearchResultItem) -> bool {
        (self.0)(result)
    }
}

impl Eq for SearchResultItem {}

impl PartialEq for SearchResultItem {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}

impl Ord for SearchResultItem {
    fn cmp(&self, other: &Self) -> Ordering {
        match self.score.total_cmp(&other.score) {
            Ordering::Equal => other.id.cmp(&self.id),
            ordering => ordering,
        }
    }
}

impl PartialOrd for SearchResultItem {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
