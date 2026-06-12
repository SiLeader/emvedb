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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Metric {
    Cosine,
    L2,
    Dot,
}

impl Metric {
    pub const fn from_u8(v: u8) -> Option<Metric> {
        match v {
            0 => Some(Metric::Cosine),
            1 => Some(Metric::L2),
            2 => Some(Metric::Dot),
            _ => None,
        }
    }

    pub const fn to_u8(self) -> u8 {
        match self {
            Metric::Cosine => 0,
            Metric::L2 => 1,
            Metric::Dot => 2,
        }
    }
}

struct SearchItem<'a> {
    id: u64,
    vector: &'a [f32],
    inv_norm: f32,
}

pub(crate) fn compute_inv_norm(vector: &[f32]) -> f32 {
    let norm_sq = vector.iter().map(|x| x * x).sum::<f32>();
    if norm_sq == 0.0 {
        0.0
    } else {
        1.0 / norm_sq.sqrt()
    }
}

pub(crate) fn dot_scoring(a: &[f32], b: &[f32]) -> f32 {
    let dot = dot(a, b);
    dot
}

pub(crate) fn l2_scoring(a: &[f32], b: &[f32]) -> f32 {
    let l2 = l2_sq(a, b);
    -l2
}

pub(crate) fn cos_scoring(a: &[f32], inv_norm_a: f32, b: &[f32], inv_norm_b: f32) -> f32 {
    if inv_norm_a == 0.0 || inv_norm_b == 0.0 {
        return f32::NEG_INFINITY;
    }
    let cos = cos(a, inv_norm_a, b, inv_norm_b);
    cos
}

fn dot(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b.iter()).map(|(x, y)| x * y).sum()
}

fn l2_sq(a: &[f32], b: &[f32]) -> f32 {
    a.iter()
        .zip(b.iter())
        .map(|(x, y)| (x - y) * (x - y))
        .sum::<f32>()
        .sqrt()
}

fn cos(a: &[f32], inv_norm_a: f32, b: &[f32], inv_norm_b: f32) -> f32 {
    dot(a, b) * inv_norm_a * inv_norm_b
}
