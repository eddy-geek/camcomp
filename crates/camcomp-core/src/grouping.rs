//! Group construction, alignment resolution and manual overrides.
//!
//! Connected components are deliberately *not* groups: compatibility is not
//! transitive, so `A–B` and `B–C` compatible with `A–C` incompatible must not
//! collapse into one unchecked component (`SPEC.md` §6). Groups are built around
//! an anchor and every accepted member is checked against the members already
//! accepted.

use std::collections::{BTreeMap, BTreeSet};

use crate::geometry::Mat3;
use crate::schema::{
    Alignment, AlignmentProvenance, CoordinateSystem, Correspondence, Group, GroupEvidence,
    GroupFlag, ManualOverride, OverrideOp, PairResult, PairState, TransformDirection, Ungrouped,
    UngroupedReason,
};
use crate::sessions::{focal_comparison, CandidatePair, FocalDescriptor};
use crate::{Error, Result};

/// Score margin below the threshold that still earns a
/// [`GroupFlag::NearThreshold`], so borderline groups reach the user without a
/// re-analysis (`SPEC.md` §5.4).
const NEAR_THRESHOLD_MARGIN: f64 = 0.05;

/// The minimum a photo needs to participate in grouping. Deliberately narrow so
/// grouping is testable without decoding an image.
#[derive(Debug, Clone, PartialEq)]
pub struct GroupingPhoto {
    /// Analysis ID.
    pub id: String,
    /// Configuration (device + mode).
    pub configuration_id: String,
    /// What the anchor-relative focal check needs.
    pub focal: FocalDescriptor,
    /// Session, when one could be assigned.
    pub session_id: Option<String>,
    /// `false` when the file could not be read or decoded.
    pub readable: bool,
}

/// Inputs to automatic group construction.
pub struct GroupingInput<'a> {
    /// Every input photo.
    pub photos: &'a [GroupingPhoto],
    /// Pair results, analyzed or skipped.
    pub pairs: &'a [PairResult],
    /// Candidate pairs, used to explain why a photo stayed ungrouped.
    pub candidates: &'a [CandidatePair],
    /// Angle-of-view tolerance for the anchor-relative membership check.
    pub field_of_view_tolerance: f64,
    /// Relaxed tolerance for a pair of native-lens photos.
    pub field_of_view_tolerance_native_lens: f64,
    /// Compatibility threshold, for the near-threshold flag.
    pub compatibility_threshold: f64,
    /// Keep at most one photo per configuration in a group.
    pub one_photo_per_configuration: bool,
    /// Anchors from a previous analysis, preferred on reload so the anchor — and
    /// therefore every persisted transform — stays stable.
    pub preferred_anchors: &'a BTreeSet<String>,
}

/// Automatic grouping output.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GroupingOutput {
    /// Groups, in construction order.
    pub groups: Vec<Group>,
    /// Photos in no group, with a reason.
    pub ungrouped: Vec<Ungrouped>,
}

/// Compatible-peer evidence for one photo.
struct Adjacency {
    /// peer id -> composite score
    peers: BTreeMap<String, f64>,
}

fn build_adjacency(pairs: &[PairResult]) -> BTreeMap<String, Adjacency> {
    let mut map: BTreeMap<String, Adjacency> = BTreeMap::new();
    for pair in pairs {
        if pair.state != PairState::Compatible {
            continue;
        }
        let score = pair.score.unwrap_or(0.0);
        map.entry(pair.a.clone())
            .or_insert_with(|| Adjacency {
                peers: BTreeMap::new(),
            })
            .peers
            .insert(pair.b.clone(), score);
        map.entry(pair.b.clone())
            .or_insert_with(|| Adjacency {
                peers: BTreeMap::new(),
            })
            .peers
            .insert(pair.a.clone(), score);
    }
    map
}

/// Build groups from pair evidence.
pub fn build_groups(input: &GroupingInput<'_>) -> GroupingOutput {
    let adjacency = build_adjacency(input.pairs);
    let by_id: BTreeMap<&str, &GroupingPhoto> =
        input.photos.iter().map(|p| (p.id.as_str(), p)).collect();
    let pair_index: BTreeMap<(&str, &str), &PairResult> = input
        .pairs
        .iter()
        .map(|p| ((p.a.as_str(), p.b.as_str()), p))
        .collect();
    let lookup_pair = |x: &str, y: &str| -> Option<&PairResult> {
        let key = if x <= y { (x, y) } else { (y, x) };
        pair_index.get(&key).copied()
    };

    // Anchor ranking: previously-an-anchor first (stability), then the largest
    // compatible set, then the strongest total evidence, then ID for determinism.
    let mut ranked: Vec<&GroupingPhoto> = input.photos.iter().collect();
    ranked.sort_by(|a, b| {
        let deg = |p: &GroupingPhoto| adjacency.get(&p.id).map(|a| a.peers.len()).unwrap_or(0);
        let total = |p: &GroupingPhoto| {
            adjacency
                .get(&p.id)
                .map(|a| a.peers.values().sum::<f64>())
                .unwrap_or(0.0)
        };
        let preferred = |p: &GroupingPhoto| input.preferred_anchors.contains(&p.id);
        preferred(b)
            .cmp(&preferred(a))
            .then(deg(b).cmp(&deg(a)))
            .then(
                total(b)
                    .partial_cmp(&total(a))
                    .unwrap_or(std::cmp::Ordering::Equal),
            )
            .then(a.id.cmp(&b.id))
    });

    let mut assigned: BTreeSet<String> = BTreeSet::new();
    let mut groups: Vec<Group> = Vec::new();
    // Why group construction turned a photo away. Recorded as it happens,
    // because reconstructing it afterwards from the pair table produces
    // plausible-but-wrong answers — a photo with a perfectly compatible peer
    // must not be reported as `all-pairs-incompatible`.
    let mut exclusion: BTreeMap<String, UngroupedReason> = BTreeMap::new();

    for anchor in ranked {
        if assigned.contains(&anchor.id) {
            continue;
        }
        let Some(adj) = adjacency.get(&anchor.id) else {
            continue;
        };

        // Candidate members: compatible with the anchor, focal-compatible with
        // the anchor, unassigned. The anchor-relative focal check is stricter
        // than the pairwise prefilter on purpose: focal compatibility is not
        // transitive (`SPEC.md` §4.2).
        let mut candidates: Vec<(String, f64)> = adj
            .peers
            .iter()
            .filter(|(id, _)| !assigned.contains(*id))
            .filter(|(id, _)| {
                let Some(peer) = by_id.get(id.as_str()) else {
                    return false;
                };
                focal_comparison(
                    &peer.focal,
                    &anchor.focal,
                    input.field_of_view_tolerance,
                    input.field_of_view_tolerance_native_lens,
                )
                .state
                    != crate::schema::FocalCompatibility::Incompatible
            })
            .map(|(id, score)| (id.clone(), *score))
            .collect();
        // Strongest evidence first, so a disagreement removes the weaker member.
        candidates.sort_by(|a, b| {
            b.1.partial_cmp(&a.1)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(a.0.cmp(&b.0))
        });

        let mut members: Vec<String> = Vec::new();
        let mut configurations: BTreeMap<String, String> = BTreeMap::new();
        if let Some(a) = by_id.get(anchor.id.as_str()) {
            configurations.insert(a.configuration_id.clone(), anchor.id.clone());
        }
        let mut incompatible_member_pairs = 0usize;
        let mut compatible_member_pairs = 0usize;
        let mut unverified_member_pairs = 0usize;

        let mut configuration_excluded = 0usize;
        for (candidate, _score) in candidates {
            let Some(photo) = by_id.get(candidate.as_str()) else {
                continue;
            };
            // At most one photo per configuration unless a human says otherwise.
            if input.one_photo_per_configuration
                && configurations.contains_key(&photo.configuration_id)
            {
                configuration_excluded += 1;
                exclusion
                    .entry(candidate.clone())
                    .or_insert(UngroupedReason::NoDistinctConfigurationPeer);
                continue;
            }
            // Check against the members already accepted, not only the anchor.
            let mut rejected = false;
            let mut local_compatible = 0usize;
            let mut local_unverified = 0usize;
            for existing in &members {
                match lookup_pair(&candidate, existing) {
                    Some(p) if p.state == PairState::Compatible => local_compatible += 1,
                    Some(p) if p.state == PairState::Incompatible => {
                        rejected = true;
                        break;
                    }
                    _ => local_unverified += 1,
                }
            }
            if rejected {
                incompatible_member_pairs += 1;
                exclusion
                    .entry(candidate.clone())
                    .or_insert(UngroupedReason::InconsistentWithGroupMembers);
                continue;
            }
            compatible_member_pairs += local_compatible;
            unverified_member_pairs += local_unverified;
            configurations.insert(photo.configuration_id.clone(), candidate.clone());
            members.push(candidate);
        }

        if members.is_empty() {
            // A group is meaningful at two members; a lone anchor is not a group.
            // The anchor is turned away for the same reason its candidates were.
            if configuration_excluded > 0 {
                exclusion
                    .entry(anchor.id.clone())
                    .or_insert(UngroupedReason::NoDistinctConfigurationPeer);
            } else if incompatible_member_pairs > 0 {
                exclusion
                    .entry(anchor.id.clone())
                    .or_insert(UngroupedReason::InconsistentWithGroupMembers);
            }
            continue;
        }

        let mut photo_ids = vec![anchor.id.clone()];
        members.sort();
        photo_ids.extend(members.iter().cloned());
        for id in &photo_ids {
            assigned.insert(id.clone());
        }

        let anchor_scores: Vec<f64> = members
            .iter()
            .filter_map(|m| lookup_pair(m, &anchor.id).and_then(|p| p.score))
            .collect();
        let mean = if anchor_scores.is_empty() {
            None
        } else {
            Some(anchor_scores.iter().sum::<f64>() / anchor_scores.len() as f64)
        };
        let min = anchor_scores
            .iter()
            .copied()
            .fold(None, |acc: Option<f64>, v| {
                Some(acc.map_or(v, |a| a.min(v)))
            });

        let mut flags: Vec<GroupFlag> = Vec::new();
        if incompatible_member_pairs > 0 {
            flags.push(GroupFlag::InternalDisagreement);
        }
        if unverified_member_pairs > 0 {
            flags.push(GroupFlag::UnverifiedPairs);
        }
        if let Some(min) = min {
            if min < input.compatibility_threshold + NEAR_THRESHOLD_MARGIN {
                flags.push(GroupFlag::NearThreshold);
            }
        }

        let group_id = format!("group-{}", anchor.id.trim_start_matches("photo-"));
        let alignments = resolve_alignments(&group_id, &anchor.id, &photo_ids, input.pairs);
        groups.push(Group {
            id: group_id,
            anchor_photo_id: anchor.id.clone(),
            photo_ids,
            alignments,
            flags,
            evidence: GroupEvidence {
                mean_anchor_score: mean,
                min_anchor_score: min,
                compatible_member_pairs,
                incompatible_member_pairs,
                unverified_member_pairs,
            },
        });
    }

    let ungrouped = explain_ungrouped(input, &assigned, &exclusion);
    GroupingOutput { groups, ungrouped }
}

/// Why each unassigned photo joined no group.
fn explain_ungrouped(
    input: &GroupingInput<'_>,
    assigned: &BTreeSet<String>,
    exclusion: &BTreeMap<String, UngroupedReason>,
) -> Vec<Ungrouped> {
    let mut out: Vec<Ungrouped> = Vec::new();
    for photo in input.photos {
        if assigned.contains(&photo.id) {
            continue;
        }
        let reason = if !photo.readable {
            UngroupedReason::UnreadableFile
        } else if photo.session_id.is_none() {
            UngroupedReason::UnusableTimestamp
        } else if let Some(recorded) = exclusion.get(&photo.id) {
            *recorded
        } else {
            let mine: Vec<&CandidatePair> = input
                .candidates
                .iter()
                .filter(|c| c.a == photo.id || c.b == photo.id)
                .collect();
            if mine.is_empty() {
                UngroupedReason::NoSessionPeer
            } else if mine
                .iter()
                .all(|c| c.focal.state == crate::schema::FocalCompatibility::Incompatible)
            {
                UngroupedReason::NoFocalCompatiblePeer
            } else {
                let states: Vec<PairState> = mine
                    .iter()
                    .filter_map(|c| {
                        input
                            .pairs
                            .iter()
                            .find(|p| (p.a == c.a && p.b == c.b) || (p.a == c.b && p.b == c.a))
                            .map(|p| p.state)
                    })
                    .collect();
                if states.contains(&PairState::Incompatible) {
                    UngroupedReason::AllPairsIncompatible
                } else {
                    UngroupedReason::AllPairsUncertain
                }
            }
        };
        out.push(Ungrouped {
            photo_id: photo.id.clone(),
            reason,
            detail: None,
        });
    }
    out.sort_by(|a, b| a.photo_id.cmp(&b.photo_id));
    out
}

/// The photo-to-anchor transform and correspondences implied by a pair record.
fn direct_alignment(
    pair: &PairResult,
    photo_id: &str,
    anchor_id: &str,
) -> Option<(Mat3, Vec<Correspondence>, f64)> {
    let raw = Mat3::from_row_major(pair.homography_a_to_b.as_deref()?)?;
    let confidence = pair.score.unwrap_or(0.0);
    if pair.a == photo_id && pair.b == anchor_id {
        Some((raw, pair.correspondences.clone(), confidence))
    } else if pair.b == photo_id && pair.a == anchor_id {
        let inverse = raw.inverse()?;
        // Swapping sides swaps the roles of the two stored points, and the
        // residual has to be recomputed because it is expressed in the units of
        // whichever image is now the anchor.
        let flipped = pair
            .correspondences
            .iter()
            .map(|c| {
                let photo = c.anchor;
                let anchor = c.photo;
                let error = inverse
                    .apply(photo)
                    .map(|p| (p[0] - anchor[0]).hypot(p[1] - anchor[1]))
                    .unwrap_or(1.0);
                Correspondence {
                    photo,
                    anchor,
                    match_quality: c.match_quality,
                    reprojection_error_normalized: if error.is_finite() { error } else { 1.0 },
                    scale: None,
                    angle: None,
                }
            })
            .collect();
        Some((inverse, flipped, confidence))
    } else {
        None
    }
}

/// Resolve one alignment, composing through another member when no direct pair
/// exists (`SPEC.md` §7.1).
fn resolve_one_alignment(
    photo_id: &str,
    anchor_id: &str,
    members: &[String],
    pairs: &[PairResult],
) -> Option<(Mat3, Vec<Correspondence>, f64, AlignmentProvenance)> {
    let find = |x: &str, y: &str| -> Option<&PairResult> {
        pairs
            .iter()
            .find(|p| (p.a == x && p.b == y) || (p.a == y && p.b == x))
    };

    if let Some(pair) = find(photo_id, anchor_id) {
        if let Some((m, c, conf)) = direct_alignment(pair, photo_id, anchor_id) {
            return Some((m, c, conf, AlignmentProvenance::DirectPair));
        }
    }

    // Compose photo -> bridge -> anchor.
    for bridge in members {
        if bridge == photo_id || bridge == anchor_id {
            continue;
        }
        let Some(first) = find(photo_id, bridge) else {
            continue;
        };
        let Some(second) = find(bridge, anchor_id) else {
            continue;
        };
        let Some((to_bridge, corr, c1)) = direct_alignment(first, photo_id, bridge) else {
            continue;
        };
        let Some((to_anchor, _, c2)) = direct_alignment(second, bridge, anchor_id) else {
            continue;
        };
        let composed = to_anchor.mul(&to_bridge).normalized();
        if composed.inverse().is_none() {
            continue;
        }
        // Remap the anchor-side point of each correspondence through the second
        // leg. Exact in the transform, approximate in so far as the composition
        // is; the provenance records that.
        let remapped: Vec<Correspondence> = corr
            .into_iter()
            .filter_map(|c| {
                let anchor = to_anchor.apply(c.anchor)?;
                let error = composed
                    .apply(c.photo)
                    .map(|p| (p[0] - anchor[0]).hypot(p[1] - anchor[1]))
                    .unwrap_or(1.0);
                Some(Correspondence {
                    photo: c.photo,
                    anchor,
                    match_quality: c.match_quality,
                    reprojection_error_normalized: if error.is_finite() { error } else { 1.0 },
                    scale: c.scale,
                    angle: c.angle,
                })
            })
            .collect();
        // Confidence degrades through a composition; the product is a blunt but
        // honest way to say so.
        return Some((composed, remapped, c1 * c2, AlignmentProvenance::Composed));
    }
    None
}

/// Resolve alignments for every non-anchor member of a group.
pub fn resolve_alignments(
    _group_id: &str,
    anchor_id: &str,
    photo_ids: &[String],
    pairs: &[PairResult],
) -> Vec<Alignment> {
    let mut out = Vec::new();
    for photo_id in photo_ids {
        if photo_id == anchor_id {
            continue;
        }
        let Some((homography, correspondences, confidence, provenance)) =
            resolve_one_alignment(photo_id, anchor_id, photo_ids, pairs)
        else {
            continue;
        };
        let Some(inverse) = homography.inverse() else {
            continue;
        };
        out.push(Alignment {
            photo_id: photo_id.clone(),
            coordinate_system: CoordinateSystem::NormalizedOriented,
            direction: TransformDirection::PhotoToAnchor,
            homography: homography.as_row_major().to_vec(),
            inverse_homography: inverse.as_row_major().to_vec(),
            correspondences,
            confidence,
            provenance,
        });
    }
    out.sort_by(|a, b| a.photo_id.cmp(&b.photo_id));
    out
}

fn add_flag(group: &mut Group, flag: GroupFlag) {
    if !group.flags.contains(&flag) {
        group.flags.push(flag);
    }
}

/// Apply manual overrides to automatic groups.
///
/// The user's decision wins over the automatic evidence, but the disagreement
/// stays visible: memberships with no supporting evidence are kept and flagged
/// rather than dropped (`SPEC.md` §6). Work happens on a clone, so a rejected
/// override cannot leave the document half-modified.
pub fn apply_overrides(
    automatic: &GroupingOutput,
    overrides: &[ManualOverride],
    photos: &[GroupingPhoto],
    pairs: &[PairResult],
) -> Result<GroupingOutput> {
    let known: BTreeSet<&str> = photos.iter().map(|p| p.id.as_str()).collect();
    let mut groups = automatic.groups.clone();
    let mut removed: BTreeMap<String, UngroupedReason> = BTreeMap::new();

    for over in overrides {
        match &over.op {
            OverrideOp::AddPhoto { group_id, photo_id } => {
                if !known.contains(photo_id.as_str()) {
                    return Err(Error::InvalidOverride(format!(
                        "`{}`: unknown photo `{photo_id}`",
                        over.id
                    )));
                }
                // Detach from wherever it currently is.
                detach(&mut groups, photo_id);
                removed.remove(photo_id);
                let group = groups
                    .iter_mut()
                    .find(|g| g.id == *group_id)
                    .ok_or_else(|| {
                        Error::InvalidOverride(format!("`{}`: unknown group `{group_id}`", over.id))
                    })?;
                if !group.photo_ids.contains(photo_id) {
                    group.photo_ids.push(photo_id.clone());
                    add_flag(group, GroupFlag::ManualMembership);
                }
            }
            OverrideOp::RemovePhoto { group_id, photo_id } => {
                let group = groups
                    .iter_mut()
                    .find(|g| g.id == *group_id)
                    .ok_or_else(|| {
                        Error::InvalidOverride(format!("`{}`: unknown group `{group_id}`", over.id))
                    })?;
                if !group.photo_ids.contains(photo_id) {
                    return Err(Error::InvalidOverride(format!(
                        "`{}`: `{photo_id}` is not a member of `{group_id}`",
                        over.id
                    )));
                }
                group.photo_ids.retain(|p| p != photo_id);
                if group.anchor_photo_id == *photo_id {
                    if let Some(next) = group.photo_ids.first().cloned() {
                        group.anchor_photo_id = next;
                        add_flag(group, GroupFlag::ManualAnchor);
                    }
                }
                removed.insert(photo_id.clone(), UngroupedReason::ManuallyRemoved);
            }
            OverrideOp::SplitGroup {
                group_id,
                photo_ids,
            } => {
                let index = groups
                    .iter()
                    .position(|g| g.id == *group_id)
                    .ok_or_else(|| {
                        Error::InvalidOverride(format!("`{}`: unknown group `{group_id}`", over.id))
                    })?;
                for pid in photo_ids {
                    if !groups[index].photo_ids.contains(pid) {
                        return Err(Error::InvalidOverride(format!(
                            "`{}`: `{pid}` is not a member of `{group_id}`",
                            over.id
                        )));
                    }
                }
                if photo_ids.len() == groups[index].photo_ids.len() {
                    return Err(Error::InvalidOverride(format!(
                        "`{}`: splitting out every member of `{group_id}` would leave it empty",
                        over.id
                    )));
                }
                let moved: Vec<String> = photo_ids.clone();
                groups[index].photo_ids.retain(|p| !moved.contains(p));
                if !groups[index]
                    .photo_ids
                    .contains(&groups[index].anchor_photo_id)
                {
                    let next = groups[index].photo_ids[0].clone();
                    groups[index].anchor_photo_id = next;
                    add_flag(&mut groups[index], GroupFlag::ManualAnchor);
                }
                let new_anchor = moved[0].clone();
                let mut new_group = Group {
                    id: format!("group-{}-split", new_anchor.trim_start_matches("photo-")),
                    anchor_photo_id: new_anchor,
                    photo_ids: moved,
                    alignments: Vec::new(),
                    flags: vec![GroupFlag::ManualAnchor],
                    evidence: GroupEvidence {
                        mean_anchor_score: None,
                        min_anchor_score: None,
                        compatible_member_pairs: 0,
                        incompatible_member_pairs: 0,
                        unverified_member_pairs: 0,
                    },
                };
                if new_group.photo_ids.len() < 2 {
                    for pid in new_group.photo_ids.drain(..) {
                        removed.insert(pid, UngroupedReason::ManuallyRemoved);
                    }
                } else {
                    groups.push(new_group);
                }
            }
            OverrideOp::MergeGroups { group_ids } => {
                if group_ids.len() < 2 {
                    return Err(Error::InvalidOverride(format!(
                        "`{}`: a merge needs at least two groups",
                        over.id
                    )));
                }
                for gid in group_ids {
                    if !groups.iter().any(|g| g.id == *gid) {
                        return Err(Error::InvalidOverride(format!(
                            "`{}`: unknown group `{gid}`",
                            over.id
                        )));
                    }
                }
                let target = group_ids[0].clone();
                let mut absorbed: Vec<String> = Vec::new();
                for gid in &group_ids[1..] {
                    if let Some(pos) = groups.iter().position(|g| g.id == *gid) {
                        let g = groups.remove(pos);
                        absorbed.extend(g.photo_ids);
                    }
                }
                let group = groups
                    .iter_mut()
                    .find(|g| g.id == target)
                    .expect("checked above");
                for pid in absorbed {
                    if !group.photo_ids.contains(&pid) {
                        group.photo_ids.push(pid);
                    }
                }
                add_flag(group, GroupFlag::ManualMembership);
            }
            OverrideOp::SetAnchor { group_id, photo_id } => {
                let group = groups
                    .iter_mut()
                    .find(|g| g.id == *group_id)
                    .ok_or_else(|| {
                        Error::InvalidOverride(format!("`{}`: unknown group `{group_id}`", over.id))
                    })?;
                if !group.photo_ids.contains(photo_id) {
                    return Err(Error::InvalidOverride(format!(
                        "`{}`: anchor `{photo_id}` must be a member of `{group_id}`",
                        over.id
                    )));
                }
                group.anchor_photo_id = photo_id.clone();
                add_flag(group, GroupFlag::ManualAnchor);
            }
            OverrideOp::AllowDuplicateConfiguration { group_id } => {
                let group = groups
                    .iter_mut()
                    .find(|g| g.id == *group_id)
                    .ok_or_else(|| {
                        Error::InvalidOverride(format!("`{}`: unknown group `{group_id}`", over.id))
                    })?;
                add_flag(group, GroupFlag::DuplicateConfiguration);
            }
        }
    }

    // Dissolve anything that fell below two members.
    let mut kept: Vec<Group> = Vec::new();
    for mut group in groups {
        if group.photo_ids.len() < 2 {
            for pid in group.photo_ids.drain(..) {
                removed
                    .entry(pid)
                    .or_insert(UngroupedReason::ManuallyRemoved);
            }
            continue;
        }
        // Keep the anchor first, then members by ID.
        let anchor = group.anchor_photo_id.clone();
        let mut rest: Vec<String> = group
            .photo_ids
            .iter()
            .filter(|p| **p != anchor)
            .cloned()
            .collect();
        rest.sort();
        group.photo_ids = std::iter::once(anchor.clone()).chain(rest).collect();

        // Recompute alignments: the anchor or the membership may have moved, and
        // a member with no usable evidence must be flagged rather than silently
        // losing its transform.
        group.alignments = resolve_alignments(&group.id, &anchor, &group.photo_ids, pairs);
        let aligned: BTreeSet<&str> = group
            .alignments
            .iter()
            .map(|a| a.photo_id.as_str())
            .collect();
        if group
            .photo_ids
            .iter()
            .any(|p| *p != anchor && !aligned.contains(p.as_str()))
        {
            add_flag(&mut group, GroupFlag::UnverifiedPairs);
            add_flag(&mut group, GroupFlag::ManualMembership);
        }
        if group
            .alignments
            .iter()
            .any(|a| a.provenance == AlignmentProvenance::Composed)
        {
            add_flag(&mut group, GroupFlag::UnverifiedPairs);
        }
        // Duplicate configurations are legal only when a human allowed them.
        let mut configs: BTreeMap<&str, usize> = BTreeMap::new();
        for pid in &group.photo_ids {
            if let Some(p) = photos.iter().find(|p| p.id == *pid) {
                *configs.entry(p.configuration_id.as_str()).or_insert(0) += 1;
            }
        }
        if configs.values().any(|c| *c > 1) {
            add_flag(&mut group, GroupFlag::DuplicateConfiguration);
        }
        kept.push(group);
    }

    // Rebuild the ungrouped list: every photo must appear exactly once overall.
    let in_group: BTreeSet<&str> = kept
        .iter()
        .flat_map(|g| g.photo_ids.iter().map(|p| p.as_str()))
        .collect();
    let mut ungrouped: Vec<Ungrouped> = Vec::new();
    for photo in photos {
        if in_group.contains(photo.id.as_str()) {
            continue;
        }
        let reason = removed
            .get(&photo.id)
            .copied()
            .or_else(|| {
                automatic
                    .ungrouped
                    .iter()
                    .find(|u| u.photo_id == photo.id)
                    .map(|u| u.reason)
            })
            .unwrap_or(UngroupedReason::ManuallyRemoved);
        ungrouped.push(Ungrouped {
            photo_id: photo.id.clone(),
            reason,
            detail: None,
        });
    }
    ungrouped.sort_by(|a, b| a.photo_id.cmp(&b.photo_id));

    Ok(GroupingOutput {
        groups: kept,
        ungrouped,
    })
}

fn detach(groups: &mut [Group], photo_id: &str) {
    for group in groups.iter_mut() {
        if group.photo_ids.iter().any(|p| p == photo_id) {
            group.photo_ids.retain(|p| p != photo_id);
            if group.anchor_photo_id == photo_id {
                if let Some(next) = group.photo_ids.first().cloned() {
                    group.anchor_photo_id = next;
                    add_flag(group, GroupFlag::ManualAnchor);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::{FocalComparison, FocalCompatibility, PairMetrics};

    fn photo(id: &str, config: &str, focal: Option<f64>) -> GroupingPhoto {
        // Native lens unless a test says otherwise: the relaxed tier only
        // widens the tolerance, so it is the case that must not leak.
        GroupingPhoto {
            id: id.to_string(),
            configuration_id: config.to_string(),
            focal: FocalDescriptor {
                effective_35mm: focal,
                native_lens: true,
            },
            session_id: Some("session-001".to_string()),
            readable: true,
        }
    }

    fn metrics() -> PairMetrics {
        PairMetrics {
            keypoints_a: 500,
            keypoints_b: 500,
            filtered_matches: 200,
            inliers: 120,
            inlier_ratio: 0.6,
            median_reprojection_error_px: 1.0,
            occupied_cells_a: 12,
            occupied_cells_b: 12,
            geometric_overlap: 0.9,
            feature_coverage: 0.8,
            inlier_confidence: 0.9,
            aligned_appearance_similarity: 0.88,
            registered_ssim: None,
            affine_scale: 1.0,
            duration_ms: 1,
        }
    }

    fn pair(a: &str, b: &str, state: PairState, score: f64, shift: f64) -> PairResult {
        let (a, b) = if a <= b { (a, b) } else { (b, a) };
        PairResult {
            id: crate::schema::canonical_pair_id(a, b),
            a: a.to_string(),
            b: b.to_string(),
            session_id: "session-001".to_string(),
            time_distance_seconds: 1,
            state,
            reason: None,
            score: Some(score),
            hard_gates_passed: state == PairState::Compatible,
            gate_failures: vec![],
            focal: FocalComparison {
                state: FocalCompatibility::Compatible,
                relative_fov_difference: Some(0.0),
                relative_focal_difference: Some(0.0),
                applied_tolerance: Some(0.10),
                native_lens_pair: true,
            },
            metrics: Some(metrics()),
            homography_a_to_b: Some(vec![1.0, 0.0, shift, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0]),
            correspondences: vec![Correspondence {
                photo: [0.25, 0.4],
                anchor: [0.25 + shift, 0.4],
                match_quality: 0.9,
                reprojection_error_normalized: 0.001,
                scale: Some(0.01),
                angle: Some(30.0),
            }],
        }
    }

    fn candidates_for(pairs: &[PairResult]) -> Vec<CandidatePair> {
        pairs
            .iter()
            .map(|p| CandidatePair {
                a: p.a.clone(),
                b: p.b.clone(),
                session_id: p.session_id.clone(),
                time_distance_seconds: p.time_distance_seconds,
                focal: p.focal.clone(),
                forced: false,
            })
            .collect()
    }

    fn input<'a>(
        photos: &'a [GroupingPhoto],
        pairs: &'a [PairResult],
        candidates: &'a [CandidatePair],
        preferred: &'a BTreeSet<String>,
    ) -> GroupingInput<'a> {
        GroupingInput {
            photos,
            pairs,
            candidates,
            field_of_view_tolerance: 0.10,
            field_of_view_tolerance_native_lens: 0.20,
            compatibility_threshold: 0.80,
            one_photo_per_configuration: true,
            preferred_anchors: preferred,
        }
    }

    #[test]
    fn two_compatible_photos_form_one_group_with_an_alignment() {
        let photos = vec![
            photo("photo-a", "oppo", Some(24.0)),
            photo("photo-b", "pixel", Some(24.0)),
        ];
        let pairs = vec![pair("photo-a", "photo-b", PairState::Compatible, 0.9, 0.02)];
        let candidates = candidates_for(&pairs);
        let out = build_groups(&input(&photos, &pairs, &candidates, &BTreeSet::new()));
        assert_eq!(out.groups.len(), 1);
        assert_eq!(out.groups[0].photo_ids.len(), 2);
        assert!(out.ungrouped.is_empty());
        assert_eq!(out.groups[0].alignments.len(), 1);
        let alignment = &out.groups[0].alignments[0];
        assert_eq!(alignment.provenance, AlignmentProvenance::DirectPair);
        assert_eq!(alignment.homography.len(), 9);
        assert_eq!(alignment.inverse_homography.len(), 9);
        assert!(!alignment.correspondences.is_empty());
    }

    #[test]
    fn a_lone_photo_is_never_a_group_of_one() {
        let photos = vec![photo("photo-a", "oppo", Some(24.0))];
        let out = build_groups(&input(&photos, &[], &[], &BTreeSet::new()));
        assert!(out.groups.is_empty());
        assert_eq!(out.ungrouped.len(), 1);
        assert_eq!(out.ungrouped[0].reason, UngroupedReason::NoSessionPeer);
    }

    #[test]
    fn non_transitive_evidence_does_not_become_one_component() {
        // a-b and b-c compatible, a-c incompatible.
        let photos = vec![
            photo("photo-a", "c1", Some(24.0)),
            photo("photo-b", "c2", Some(24.0)),
            photo("photo-c", "c3", Some(24.0)),
        ];
        let pairs = vec![
            pair("photo-a", "photo-b", PairState::Compatible, 0.92, 0.02),
            pair("photo-b", "photo-c", PairState::Compatible, 0.90, 0.03),
            pair("photo-a", "photo-c", PairState::Incompatible, 0.40, 0.0),
        ];
        let candidates = candidates_for(&pairs);
        let out = build_groups(&input(&photos, &pairs, &candidates, &BTreeSet::new()));
        // Whatever the anchor, no group may contain both a and c.
        for group in &out.groups {
            let has_a = group.photo_ids.iter().any(|p| p == "photo-a");
            let has_c = group.photo_ids.iter().any(|p| p == "photo-c");
            assert!(!(has_a && has_c), "a and c were merged: {group:?}");
        }
        let total: usize = out.groups.iter().map(|g| g.photo_ids.len()).sum();
        assert_eq!(
            total + out.ungrouped.len(),
            3,
            "every photo must appear once"
        );
    }

    #[test]
    fn a_disagreeing_member_is_flagged() {
        let photos = vec![
            photo("photo-a", "c1", Some(24.0)),
            photo("photo-b", "c2", Some(24.0)),
            photo("photo-c", "c3", Some(24.0)),
        ];
        let pairs = vec![
            pair("photo-b", "photo-a", PairState::Compatible, 0.95, 0.02),
            pair("photo-b", "photo-c", PairState::Compatible, 0.90, 0.03),
            pair("photo-a", "photo-c", PairState::Incompatible, 0.30, 0.0),
        ];
        let candidates = candidates_for(&pairs);
        let out = build_groups(&input(&photos, &pairs, &candidates, &BTreeSet::new()));
        let group = out
            .groups
            .iter()
            .find(|g| g.anchor_photo_id == "photo-b")
            .expect("photo-b has the largest compatible set");
        assert!(group.flags.contains(&GroupFlag::InternalDisagreement));
        assert_eq!(group.evidence.incompatible_member_pairs, 1);
    }

    #[test]
    fn only_one_photo_per_configuration_is_taken() {
        let photos = vec![
            photo("photo-a", "oppo", Some(24.0)),
            photo("photo-b", "oppo", Some(24.0)),
            photo("photo-c", "pixel", Some(24.0)),
        ];
        let pairs = vec![
            pair("photo-a", "photo-b", PairState::Compatible, 0.9, 0.01),
            pair("photo-a", "photo-c", PairState::Compatible, 0.95, 0.02),
            pair("photo-b", "photo-c", PairState::Compatible, 0.9, 0.02),
        ];
        let candidates = candidates_for(&pairs);
        let out = build_groups(&input(&photos, &pairs, &candidates, &BTreeSet::new()));
        for group in &out.groups {
            let configs: Vec<&str> = group
                .photo_ids
                .iter()
                .map(|p| {
                    photos
                        .iter()
                        .find(|x| x.id == *p)
                        .unwrap()
                        .configuration_id
                        .as_str()
                })
                .collect();
            let unique: BTreeSet<&&str> = configs.iter().collect();
            assert_eq!(configs.len(), unique.len(), "{group:?}");
        }
    }

    #[test]
    fn an_anchor_incompatible_focal_length_excludes_a_member() {
        let photos = vec![
            photo("photo-a", "c1", Some(24.0)),
            photo("photo-b", "c2", Some(70.0)),
        ];
        // The pair claims visual compatibility, but the anchor-relative focal
        // check is a separate constraint.
        let pairs = vec![pair("photo-a", "photo-b", PairState::Compatible, 0.9, 0.02)];
        let candidates = candidates_for(&pairs);
        let out = build_groups(&input(&photos, &pairs, &candidates, &BTreeSet::new()));
        assert!(out.groups.is_empty());
        assert_eq!(out.ungrouped.len(), 2);
    }

    #[test]
    fn group_membership_is_stable_under_input_permutation() {
        let mut photos = vec![
            photo("photo-a", "c1", Some(24.0)),
            photo("photo-b", "c2", Some(24.0)),
            photo("photo-c", "c3", Some(24.0)),
        ];
        let pairs = vec![
            pair("photo-a", "photo-b", PairState::Compatible, 0.9, 0.02),
            pair("photo-a", "photo-c", PairState::Compatible, 0.9, 0.03),
            pair("photo-b", "photo-c", PairState::Compatible, 0.9, 0.01),
        ];
        let candidates = candidates_for(&pairs);
        let first = build_groups(&input(&photos, &pairs, &candidates, &BTreeSet::new()));
        photos.reverse();
        let second = build_groups(&input(&photos, &pairs, &candidates, &BTreeSet::new()));
        assert_eq!(first.groups, second.groups);
    }

    #[test]
    fn a_preferred_anchor_keeps_the_anchor_stable_across_a_reload() {
        let photos = vec![
            photo("photo-a", "c1", Some(24.0)),
            photo("photo-b", "c2", Some(24.0)),
        ];
        let pairs = vec![pair("photo-a", "photo-b", PairState::Compatible, 0.9, 0.02)];
        let candidates = candidates_for(&pairs);

        let cold = build_groups(&input(&photos, &pairs, &candidates, &BTreeSet::new()));
        assert_eq!(cold.groups[0].anchor_photo_id, "photo-a");
        // The stored pair maps a -> b with +0.02; into anchor a, b maps by -0.02.
        let cold_h = Mat3::from_row_major(&cold.groups[0].alignments[0].homography).unwrap();
        assert_eq!(cold.groups[0].alignments[0].photo_id, "photo-b");
        assert!((cold_h.apply([0.5, 0.5]).unwrap()[0] - 0.48).abs() < 1e-9);

        let preferred: BTreeSet<String> = ["photo-b".to_string()].into_iter().collect();
        let warm = build_groups(&input(&photos, &pairs, &candidates, &preferred));
        assert_eq!(warm.groups[0].anchor_photo_id, "photo-b");
        // The alignment must be re-derived for the new anchor, not reused.
        assert_eq!(warm.groups[0].alignments[0].photo_id, "photo-a");
        let warm_h = Mat3::from_row_major(&warm.groups[0].alignments[0].homography).unwrap();
        assert!((warm_h.apply([0.5, 0.5]).unwrap()[0] - 0.52).abs() < 1e-9);
    }

    #[test]
    fn ungrouped_reasons_distinguish_the_failure_modes() {
        let mut photos = vec![
            photo("photo-a", "c1", Some(24.0)),
            photo("photo-b", "c2", Some(24.0)),
            photo("photo-c", "c3", Some(24.0)),
            photo("photo-d", "c4", Some(24.0)),
        ];
        photos[2].session_id = None;
        photos[3].readable = false;
        let pairs = vec![pair(
            "photo-a",
            "photo-b",
            PairState::Incompatible,
            0.3,
            0.0,
        )];
        let candidates = candidates_for(&pairs);
        let out = build_groups(&input(&photos, &pairs, &candidates, &BTreeSet::new()));
        let reason = |id: &str| {
            out.ungrouped
                .iter()
                .find(|u| u.photo_id == id)
                .unwrap()
                .reason
        };
        assert_eq!(reason("photo-a"), UngroupedReason::AllPairsIncompatible);
        assert_eq!(reason("photo-b"), UngroupedReason::AllPairsIncompatible);
        assert_eq!(reason("photo-c"), UngroupedReason::UnusableTimestamp);
        assert_eq!(reason("photo-d"), UngroupedReason::UnreadableFile);
    }

    #[test]
    fn an_all_uncertain_photo_says_so() {
        let photos = vec![
            photo("photo-a", "c1", Some(24.0)),
            photo("photo-b", "c2", Some(24.0)),
        ];
        let pairs = vec![pair("photo-a", "photo-b", PairState::Uncertain, 0.5, 0.0)];
        let candidates = candidates_for(&pairs);
        let out = build_groups(&input(&photos, &pairs, &candidates, &BTreeSet::new()));
        assert_eq!(out.ungrouped[0].reason, UngroupedReason::AllPairsUncertain);
    }

    #[test]
    fn a_focal_only_rejection_says_so() {
        let photos = vec![
            photo("photo-a", "c1", Some(24.0)),
            photo("photo-b", "c2", Some(70.0)),
        ];
        let candidates = vec![CandidatePair {
            a: "photo-a".into(),
            b: "photo-b".into(),
            session_id: "session-001".into(),
            time_distance_seconds: 1,
            focal: FocalComparison {
                state: FocalCompatibility::Incompatible,
                relative_fov_difference: Some(0.9),
                relative_focal_difference: Some(1.9),
                applied_tolerance: Some(0.10),
                native_lens_pair: true,
            },
            forced: false,
        }];
        let out = build_groups(&input(&photos, &[], &candidates, &BTreeSet::new()));
        assert_eq!(
            out.ungrouped[0].reason,
            UngroupedReason::NoFocalCompatiblePeer
        );
    }

    // ---- Manual overrides ---------------------------------------------------

    fn over(id: &str, op: OverrideOp) -> ManualOverride {
        ManualOverride {
            id: id.to_string(),
            op,
            created_at: None,
            note: None,
        }
    }

    fn three_photo_setup() -> (Vec<GroupingPhoto>, Vec<PairResult>, GroupingOutput) {
        let photos = vec![
            photo("photo-a", "c1", Some(24.0)),
            photo("photo-b", "c2", Some(24.0)),
            photo("photo-c", "c3", Some(24.0)),
        ];
        let pairs = vec![
            pair("photo-a", "photo-b", PairState::Compatible, 0.9, 0.02),
            pair("photo-a", "photo-c", PairState::Uncertain, 0.5, 0.05),
            pair("photo-b", "photo-c", PairState::Uncertain, 0.5, 0.04),
        ];
        let candidates = candidates_for(&pairs);
        let automatic = build_groups(&input(&photos, &pairs, &candidates, &BTreeSet::new()));
        (photos, pairs, automatic)
    }

    #[test]
    fn a_manually_added_member_is_kept_and_flagged() {
        let (photos, pairs, automatic) = three_photo_setup();
        assert_eq!(automatic.groups.len(), 1);
        assert_eq!(automatic.ungrouped.len(), 1);
        let group_id = automatic.groups[0].id.clone();
        let out = apply_overrides(
            &automatic,
            &[over(
                "override-1",
                OverrideOp::AddPhoto {
                    group_id,
                    photo_id: "photo-c".into(),
                },
            )],
            &photos,
            &pairs,
        )
        .unwrap();
        assert_eq!(out.groups[0].photo_ids.len(), 3);
        assert!(out.groups[0].flags.contains(&GroupFlag::ManualMembership));
        assert!(out.ungrouped.is_empty());
        // The uncertain pair still carries a transform, so an alignment exists.
        assert_eq!(out.groups[0].alignments.len(), 2);
    }

    #[test]
    fn a_member_with_no_evidence_at_all_is_retained_and_flagged() {
        let photos = vec![
            photo("photo-a", "c1", Some(24.0)),
            photo("photo-b", "c2", Some(24.0)),
            photo("photo-z", "c9", Some(24.0)),
        ];
        let pairs = vec![pair("photo-a", "photo-b", PairState::Compatible, 0.9, 0.02)];
        let candidates = candidates_for(&pairs);
        let automatic = build_groups(&input(&photos, &pairs, &candidates, &BTreeSet::new()));
        let group_id = automatic.groups[0].id.clone();
        let out = apply_overrides(
            &automatic,
            &[over(
                "override-1",
                OverrideOp::AddPhoto {
                    group_id,
                    photo_id: "photo-z".into(),
                },
            )],
            &photos,
            &pairs,
        )
        .unwrap();
        assert_eq!(out.groups[0].photo_ids.len(), 3);
        assert!(out.groups[0].flags.contains(&GroupFlag::UnverifiedPairs));
        // No transform could be derived, so no alignment is invented for it.
        assert!(out.groups[0]
            .alignments
            .iter()
            .all(|a| a.photo_id != "photo-z"));
    }

    #[test]
    fn removing_a_member_dissolves_a_pair_group() {
        let (photos, pairs, automatic) = three_photo_setup();
        let group_id = automatic.groups[0].id.clone();
        let out = apply_overrides(
            &automatic,
            &[over(
                "override-1",
                OverrideOp::RemovePhoto {
                    group_id,
                    photo_id: "photo-b".into(),
                },
            )],
            &photos,
            &pairs,
        )
        .unwrap();
        assert!(out.groups.is_empty());
        assert_eq!(out.ungrouped.len(), 3);
        assert!(out
            .ungrouped
            .iter()
            .any(|u| u.photo_id == "photo-b" && u.reason == UngroupedReason::ManuallyRemoved));
    }

    #[test]
    fn re_anchoring_recomputes_the_transforms() {
        let (photos, pairs, automatic) = three_photo_setup();
        let group_id = automatic.groups[0].id.clone();
        assert_eq!(automatic.groups[0].anchor_photo_id, "photo-a");
        let out = apply_overrides(
            &automatic,
            &[over(
                "override-1",
                OverrideOp::SetAnchor {
                    group_id,
                    photo_id: "photo-b".into(),
                },
            )],
            &photos,
            &pairs,
        )
        .unwrap();
        assert_eq!(out.groups[0].anchor_photo_id, "photo-b");
        assert!(out.groups[0].flags.contains(&GroupFlag::ManualAnchor));
        let alignment = &out.groups[0].alignments[0];
        assert_eq!(alignment.photo_id, "photo-a");
        let h = Mat3::from_row_major(&alignment.homography).unwrap();
        assert!((h.apply([0.5, 0.5]).unwrap()[0] - 0.52).abs() < 1e-9);
    }

    #[test]
    fn splitting_and_merging_round_trip() {
        let photos = vec![
            photo("photo-a", "c1", Some(24.0)),
            photo("photo-b", "c2", Some(24.0)),
            photo("photo-c", "c3", Some(24.0)),
            photo("photo-d", "c4", Some(24.0)),
        ];
        let pairs = vec![
            pair("photo-a", "photo-b", PairState::Compatible, 0.95, 0.01),
            pair("photo-a", "photo-c", PairState::Compatible, 0.94, 0.02),
            pair("photo-a", "photo-d", PairState::Compatible, 0.93, 0.03),
            pair("photo-b", "photo-c", PairState::Compatible, 0.92, 0.01),
            pair("photo-b", "photo-d", PairState::Compatible, 0.91, 0.02),
            pair("photo-c", "photo-d", PairState::Compatible, 0.90, 0.01),
        ];
        let candidates = candidates_for(&pairs);
        let automatic = build_groups(&input(&photos, &pairs, &candidates, &BTreeSet::new()));
        assert_eq!(automatic.groups.len(), 1);
        assert_eq!(automatic.groups[0].photo_ids.len(), 4);
        let group_id = automatic.groups[0].id.clone();

        let split = apply_overrides(
            &automatic,
            &[over(
                "override-1",
                OverrideOp::SplitGroup {
                    group_id: group_id.clone(),
                    photo_ids: vec!["photo-c".into(), "photo-d".into()],
                },
            )],
            &photos,
            &pairs,
        )
        .unwrap();
        assert_eq!(split.groups.len(), 2);
        assert!(split.ungrouped.is_empty());

        let ids: Vec<String> = split.groups.iter().map(|g| g.id.clone()).collect();
        let merged = apply_overrides(
            &split,
            &[over(
                "override-2",
                OverrideOp::MergeGroups { group_ids: ids },
            )],
            &photos,
            &pairs,
        )
        .unwrap();
        assert_eq!(merged.groups.len(), 1);
        assert_eq!(merged.groups[0].photo_ids.len(), 4);
    }

    #[test]
    fn a_duplicate_configuration_is_flagged_when_a_human_creates_one() {
        let photos = vec![
            photo("photo-a", "oppo", Some(24.0)),
            photo("photo-b", "pixel", Some(24.0)),
            photo("photo-c", "oppo", Some(24.0)),
        ];
        let pairs = vec![
            pair("photo-a", "photo-b", PairState::Compatible, 0.95, 0.01),
            pair("photo-a", "photo-c", PairState::Compatible, 0.94, 0.02),
        ];
        let candidates = candidates_for(&pairs);
        let automatic = build_groups(&input(&photos, &pairs, &candidates, &BTreeSet::new()));
        let group_id = automatic.groups[0].id.clone();
        let out = apply_overrides(
            &automatic,
            &[over(
                "override-1",
                OverrideOp::AddPhoto {
                    group_id,
                    photo_id: "photo-c".into(),
                },
            )],
            &photos,
            &pairs,
        )
        .unwrap();
        assert!(out.groups[0]
            .flags
            .contains(&GroupFlag::DuplicateConfiguration));
    }

    #[test]
    fn invalid_overrides_are_rejected_without_touching_the_document() {
        let (photos, pairs, automatic) = three_photo_setup();
        let bad = [
            over(
                "override-1",
                OverrideOp::AddPhoto {
                    group_id: "group-nope".into(),
                    photo_id: "photo-c".into(),
                },
            ),
            over(
                "override-2",
                OverrideOp::AddPhoto {
                    group_id: automatic.groups[0].id.clone(),
                    photo_id: "photo-nope".into(),
                },
            ),
            over(
                "override-3",
                OverrideOp::SetAnchor {
                    group_id: automatic.groups[0].id.clone(),
                    photo_id: "photo-c".into(),
                },
            ),
            over(
                "override-4",
                OverrideOp::RemovePhoto {
                    group_id: automatic.groups[0].id.clone(),
                    photo_id: "photo-c".into(),
                },
            ),
            over(
                "override-5",
                OverrideOp::MergeGroups {
                    group_ids: vec![automatic.groups[0].id.clone()],
                },
            ),
            over(
                "override-6",
                OverrideOp::SplitGroup {
                    group_id: automatic.groups[0].id.clone(),
                    photo_ids: automatic.groups[0].photo_ids.clone(),
                },
            ),
        ];
        for op in bad {
            let err = apply_overrides(&automatic, std::slice::from_ref(&op), &photos, &pairs)
                .unwrap_err();
            assert_eq!(err.code(), "invalid-override", "{op:?}");
        }
        // The automatic result is untouched.
        assert_eq!(automatic.groups.len(), 1);
        assert_eq!(automatic.groups[0].photo_ids.len(), 2);
    }

    #[test]
    fn every_photo_appears_exactly_once_after_overrides() {
        let (photos, pairs, automatic) = three_photo_setup();
        let group_id = automatic.groups[0].id.clone();
        let out = apply_overrides(
            &automatic,
            &[
                over(
                    "override-1",
                    OverrideOp::AddPhoto {
                        group_id: group_id.clone(),
                        photo_id: "photo-c".into(),
                    },
                ),
                over(
                    "override-2",
                    OverrideOp::RemovePhoto {
                        group_id,
                        photo_id: "photo-a".into(),
                    },
                ),
            ],
            &photos,
            &pairs,
        )
        .unwrap();
        let mut seen: Vec<&str> = out
            .groups
            .iter()
            .flat_map(|g| g.photo_ids.iter().map(|p| p.as_str()))
            .chain(out.ungrouped.iter().map(|u| u.photo_id.as_str()))
            .collect();
        seen.sort_unstable();
        assert_eq!(seen, vec!["photo-a", "photo-b", "photo-c"]);
    }

    #[test]
    fn an_alignment_can_be_composed_through_a_bridge_member() {
        // b has no direct pair with the anchor a, but does with c.
        let pairs = vec![
            pair("photo-a", "photo-c", PairState::Compatible, 0.95, 0.10),
            pair("photo-b", "photo-c", PairState::Compatible, 0.94, 0.05),
        ];
        let alignments = resolve_alignments(
            "group-a",
            "photo-a",
            &[
                "photo-a".to_string(),
                "photo-b".to_string(),
                "photo-c".to_string(),
            ],
            &pairs,
        );
        let b = alignments.iter().find(|a| a.photo_id == "photo-b").unwrap();
        assert_eq!(b.provenance, AlignmentProvenance::Composed);
        let h = Mat3::from_row_major(&b.homography).unwrap();
        // b -> c is +0.05, c -> a is -0.10, so b -> a is -0.05.
        assert!(
            (h.apply([0.5, 0.5]).unwrap()[0] - 0.45).abs() < 1e-9,
            "{h:?}"
        );
        assert!(b.confidence < 0.95);
    }
}
