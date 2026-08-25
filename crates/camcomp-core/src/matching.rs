//! Deterministic brute-force descriptor matching.
//!
//! Small enough to own rather than delegate (`ADR-DROP1.md` §3.6). The
//! properties that matter are that equal distances break ties by index, so the
//! result never depends on iteration order, and that the ratio test and the
//! mutual check are separable so each can be tested on its own.

use crate::domain::{DescriptorMatcher, FeatureMatch, PhotoFeatures, DESCRIPTOR_LEN};

/// Brute-force two-nearest-neighbour matcher with Lowe ratio filtering and an
/// optional mutual-agreement check.
#[derive(Debug, Clone, Copy)]
pub struct BruteForceMatcher {
    /// Lowe ratio threshold. A match survives when
    /// `nearest / second_nearest <= ratio`.
    pub lowe_ratio: f64,
    /// Require that `b`'s nearest neighbour is `a` as well.
    pub mutual: bool,
}

impl Default for BruteForceMatcher {
    fn default() -> Self {
        Self {
            lowe_ratio: 0.75,
            mutual: true,
        }
    }
}

/// Squared L2 distance between two descriptors.
///
/// Squared rather than Euclidean: monotonic in the same order, and the ratio
/// test is applied to the square roots so the threshold keeps its usual meaning.
#[inline]
pub fn descriptor_distance_squared(a: &[u8; DESCRIPTOR_LEN], b: &[u8; DESCRIPTOR_LEN]) -> u32 {
    let mut acc: u32 = 0;
    for i in 0..DESCRIPTOR_LEN {
        let d = a[i] as i32 - b[i] as i32;
        acc += (d * d) as u32;
    }
    acc
}

/// The two nearest neighbours of `query` in `haystack`.
///
/// Returns `(best_index, best_distance², second_distance²)`. Ties keep the lower
/// index, which is what makes the whole pipeline order-independent.
pub fn two_nearest(
    query: &[u8; DESCRIPTOR_LEN],
    haystack: &[[u8; DESCRIPTOR_LEN]],
) -> Option<(usize, u32, Option<u32>)> {
    if haystack.is_empty() {
        return None;
    }
    let mut best = (0usize, u32::MAX);
    let mut second = u32::MAX;
    for (i, candidate) in haystack.iter().enumerate() {
        let d = descriptor_distance_squared(query, candidate);
        if d < best.1 {
            second = best.1;
            best = (i, d);
        } else if d < second {
            second = d;
        }
    }
    let second = if haystack.len() >= 2 && second != u32::MAX {
        Some(second)
    } else {
        None
    };
    Some((best.0, best.1, second))
}

impl DescriptorMatcher for BruteForceMatcher {
    fn match_descriptors(&self, a: &PhotoFeatures, b: &PhotoFeatures) -> Vec<FeatureMatch> {
        if a.descriptors.is_empty() || b.descriptors.is_empty() {
            return Vec::new();
        }

        let reverse_best: Option<Vec<usize>> = if self.mutual {
            Some(
                b.descriptors
                    .iter()
                    .map(|d| {
                        two_nearest(d, &a.descriptors)
                            .map(|(i, _, _)| i)
                            .unwrap_or(usize::MAX)
                    })
                    .collect(),
            )
        } else {
            None
        };

        let mut matches = Vec::new();
        for (ai, da) in a.descriptors.iter().enumerate() {
            let Some((bi, best, second)) = two_nearest(da, &b.descriptors) else {
                continue;
            };
            let ratio = match second {
                // With a single candidate there is nothing to compare against;
                // treating that as a perfect match would be an invention.
                None => continue,
                Some(second) => {
                    if second == 0 {
                        // Two identical descriptors: entirely ambiguous.
                        continue;
                    }
                    (best as f64).sqrt() / (second as f64).sqrt()
                }
            };
            if ratio > self.lowe_ratio {
                continue;
            }
            if let Some(reverse) = &reverse_best {
                if reverse[bi] != ai {
                    continue;
                }
            }
            matches.push(FeatureMatch {
                a: ai,
                b: bi,
                distance: best as f32,
                ratio: ratio as f32,
            });
        }
        // Stable, value-based ordering so RANSAC sees the same input sequence on
        // every run regardless of how the descriptors were produced.
        matches.sort_by(|x, y| {
            x.ratio
                .partial_cmp(&y.ratio)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(x.a.cmp(&y.a))
                .then(x.b.cmp(&y.b))
        });
        matches
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::Keypoint;

    fn descriptor(fill: u8, first: u8) -> [u8; DESCRIPTOR_LEN] {
        let mut d = [fill; DESCRIPTOR_LEN];
        d[0] = first;
        d
    }

    fn features(descriptors: Vec<[u8; DESCRIPTOR_LEN]>) -> PhotoFeatures {
        let keypoints = (0..descriptors.len())
            .map(|i| Keypoint {
                x: i as f32,
                y: 0.0,
                size: 2.0,
                angle: 0.0,
                response: 1.0,
            })
            .collect();
        PhotoFeatures {
            keypoints,
            descriptors,
            analysis_dimensions: (100, 100),
            duration_ms: 0,
        }
    }

    #[test]
    fn distance_is_zero_for_identical_descriptors() {
        let d = descriptor(3, 9);
        assert_eq!(descriptor_distance_squared(&d, &d), 0);
    }

    #[test]
    fn distance_is_symmetric_and_additive() {
        let a = descriptor(0, 0);
        let b = descriptor(0, 10);
        assert_eq!(descriptor_distance_squared(&a, &b), 100);
        assert_eq!(
            descriptor_distance_squared(&a, &b),
            descriptor_distance_squared(&b, &a)
        );
    }

    #[test]
    fn equal_distances_break_ties_by_lower_index() {
        let query = descriptor(0, 0);
        let haystack = vec![descriptor(0, 5), descriptor(0, 5), descriptor(0, 90)];
        let (best, d, second) = two_nearest(&query, &haystack).unwrap();
        assert_eq!(best, 0, "tie must resolve to the lower index");
        assert_eq!(d, 25);
        assert_eq!(second, Some(25));
    }

    #[test]
    fn two_nearest_of_an_empty_haystack_is_none() {
        assert!(two_nearest(&descriptor(0, 0), &[]).is_none());
    }

    #[test]
    fn a_single_candidate_has_no_second_neighbour() {
        let (_, _, second) = two_nearest(&descriptor(0, 0), &[descriptor(0, 4)]).unwrap();
        assert!(second.is_none());
    }

    #[test]
    fn the_ratio_test_boundary_is_inclusive() {
        // nearest = 6, second = 12 -> ratio 0.5.
        let a = features(vec![descriptor(0, 0)]);
        let b = features(vec![descriptor(0, 6), descriptor(0, 12)]);

        let strict = BruteForceMatcher {
            lowe_ratio: 0.5,
            mutual: false,
        };
        assert_eq!(strict.match_descriptors(&a, &b).len(), 1);

        let tighter = BruteForceMatcher {
            lowe_ratio: 0.49,
            mutual: false,
        };
        assert_eq!(tighter.match_descriptors(&a, &b).len(), 0);
    }

    #[test]
    fn an_ambiguous_match_is_dropped_not_accepted() {
        // Two identical candidates: the ratio is 1.0 and the match means nothing.
        let a = features(vec![descriptor(0, 0)]);
        let b = features(vec![descriptor(0, 7), descriptor(0, 7)]);
        let matcher = BruteForceMatcher {
            lowe_ratio: 0.99,
            mutual: false,
        };
        assert!(matcher.match_descriptors(&a, &b).is_empty());
    }

    #[test]
    fn a_perfect_duplicate_pair_is_dropped_rather_than_dividing_by_zero() {
        let a = features(vec![descriptor(0, 0)]);
        let b = features(vec![descriptor(0, 0), descriptor(0, 0)]);
        let matcher = BruteForceMatcher {
            lowe_ratio: 0.99,
            mutual: false,
        };
        assert!(matcher.match_descriptors(&a, &b).is_empty());
    }

    #[test]
    fn the_mutual_check_removes_one_sided_matches() {
        // a0 prefers b0; b0 prefers a1, which is closer to it.
        let a = features(vec![descriptor(0, 0), descriptor(0, 20)]);
        let b = features(vec![descriptor(0, 21), descriptor(0, 200)]);

        let loose = BruteForceMatcher {
            lowe_ratio: 0.95,
            mutual: false,
        };
        let mutual = BruteForceMatcher {
            lowe_ratio: 0.95,
            mutual: true,
        };
        let loose_matches = loose.match_descriptors(&a, &b);
        let mutual_matches = mutual.match_descriptors(&a, &b);
        assert!(mutual_matches.len() < loose_matches.len());
        assert!(mutual_matches.iter().all(|m| m.a == 1));
    }

    #[test]
    fn matching_is_independent_of_descriptor_order() {
        let a = features(vec![
            descriptor(1, 10),
            descriptor(2, 40),
            descriptor(3, 80),
        ]);
        let b = features(vec![
            descriptor(1, 11),
            descriptor(2, 41),
            descriptor(3, 200),
        ]);
        let matcher = BruteForceMatcher::default();
        let forward = matcher.match_descriptors(&a, &b);

        // Reverse both sides and remap indices: the matched descriptor pairs must
        // be the same set.
        let mut ra = a.clone();
        ra.descriptors.reverse();
        ra.keypoints.reverse();
        let mut rb = b.clone();
        rb.descriptors.reverse();
        rb.keypoints.reverse();
        let reversed = matcher.match_descriptors(&ra, &rb);

        let mut forward_pairs: Vec<(usize, usize)> = forward.iter().map(|m| (m.a, m.b)).collect();
        let mut reversed_pairs: Vec<(usize, usize)> = reversed
            .iter()
            .map(|m| (a.descriptors.len() - 1 - m.a, b.descriptors.len() - 1 - m.b))
            .collect();
        forward_pairs.sort_unstable();
        reversed_pairs.sort_unstable();
        assert_eq!(forward_pairs, reversed_pairs);
    }

    #[test]
    fn empty_inputs_produce_no_matches() {
        let empty = features(vec![]);
        let some = features(vec![descriptor(0, 0), descriptor(0, 5)]);
        let matcher = BruteForceMatcher::default();
        assert!(matcher.match_descriptors(&empty, &some).is_empty());
        assert!(matcher.match_descriptors(&some, &empty).is_empty());
    }

    #[test]
    fn matches_are_sorted_most_distinctive_first() {
        let a = features(vec![descriptor(0, 0), descriptor(0, 100)]);
        let b = features(vec![
            descriptor(0, 1),
            descriptor(0, 101),
            descriptor(0, 250),
        ]);
        let matches = BruteForceMatcher {
            lowe_ratio: 0.95,
            mutual: false,
        }
        .match_descriptors(&a, &b);
        for w in matches.windows(2) {
            assert!(w[0].ratio <= w[1].ratio);
        }
    }
}
