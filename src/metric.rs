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

/// Distance or similarity metric used by search.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Metric {
    /// Cosine similarity.
    ///
    /// Search scores are cosine similarities and distances are `1 - score`.
    Cosine,
    /// Euclidean distance.
    ///
    /// Search scores are negative Euclidean distances so that larger scores are
    /// better.
    L2,
    /// Dot product similarity.
    ///
    /// Search scores and distances are both the dot product value.
    Dot,
}

impl Metric {
    /// Decodes a metric from its storage representation.
    pub const fn from_u8(v: u8) -> Option<Metric> {
        match v {
            0 => Some(Metric::Cosine),
            1 => Some(Metric::L2),
            2 => Some(Metric::Dot),
            _ => None,
        }
    }

    /// Encodes the metric for storage.
    pub const fn to_u8(self) -> u8 {
        match self {
            Metric::Cosine => 0,
            Metric::L2 => 1,
            Metric::Dot => 2,
        }
    }
}

pub(crate) fn compute_inv_norm(vector: &[f32]) -> f32 {
    let norm_sq = vector.iter().map(|x| x * x).sum::<f32>();
    if norm_sq == 0.0 {
        0.0
    } else {
        1.0 / norm_sq.sqrt()
    }
}

pub(crate) fn dot_score_and_distance(a: &[f32], b: &[f32]) -> (f32, f32) {
    let dot = dot(a, b);
    (dot, dot)
}

pub(crate) fn l2_score_and_distance(a: &[f32], b: &[f32]) -> (f32, f32) {
    let distance = l2_sq(a, b).sqrt();
    (-distance, distance)
}

pub(crate) fn cos_score_and_distance(
    a: &[f32],
    inv_norm_a: f32,
    b: &[f32],
    inv_norm_b: f32,
) -> (f32, f32) {
    if inv_norm_a == 0.0 || inv_norm_b == 0.0 {
        return (f32::NEG_INFINITY, f32::INFINITY);
    }
    let cos = cos(a, inv_norm_a, b, inv_norm_b);
    (cos, 1.0 - cos)
}

fn dot(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b.iter()).map(|(x, y)| x * y).sum()
}

fn l2_sq(a: &[f32], b: &[f32]) -> f32 {
    a.iter()
        .zip(b.iter())
        .map(|(x, y)| (x - y) * (x - y))
        .sum::<f32>()
}

fn cos(a: &[f32], inv_norm_a: f32, b: &[f32], inv_norm_b: f32) -> f32 {
    dot(a, b) * inv_norm_a * inv_norm_b
}
