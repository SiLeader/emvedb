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

#[derive(Debug, Clone, Copy)]
pub struct SearchResultItem {
    pub id: u64,
    pub score: f32,
    pub distance: f32,
}

#[derive(Default)]
pub struct SearchOptions {
    pub(crate) filter: Option<Box<dyn SearchFilter>>,
    pub(crate) min_score: Option<f32>,
}

impl SearchOptions {
    pub fn with_filter<F>(mut self, filter: F) -> Self
    where
        F: SearchFilter + 'static,
    {
        self.filter = Some(Box::new(filter));
        self
    }

    pub fn with_result_filter<F>(self, filter: F) -> Self
    where
        F: Fn(&SearchResultItem) -> bool + Send + Sync + 'static,
    {
        self.with_filter(ResultFilter(filter))
    }

    pub fn with_min_score(mut self, min_score: f32) -> Self {
        self.min_score = Some(min_score);
        self
    }
}

pub trait SearchFilter: Send + Sync {
    fn filter_id(&self, _id: u64) -> bool {
        true
    }

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
