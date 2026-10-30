//! Horizontal spacing of one measure: where each point in time sits, shared by all staves.

use std::collections::BTreeMap;

use notula_model::{Score, Staff, TICKS_PER_QUARTER};

use crate::engrave::{StaffPlan, plan_staff};
use crate::{emit, smufl, volume};

const START_PAD: f32 = 1.0;
const MIN_CLEARANCE: f32 = 1.0;
const QUARTER_SPACING: f32 = 3.5;
const SPACING_EXPONENT: f32 = 0.6;
const MIN_BODY_WIDTH: f32 = 6.0;
/// Width of the final double barline: thick line, gap, thin line.
const FINAL_BARLINE_ROOM: f32 =
    smufl::THICK_BARLINE + smufl::FINAL_BARLINE_GAP + smufl::THIN_BARLINE;

pub(crate) struct MeasureMetrics {
    /// Onsets that have something to draw, ascending.
    pub columns: Vec<u32>,
    /// Distance of each column from the start of the measure body.
    pub offsets: Vec<f32>,
    pub body_width: f32,
    /// One plan per staff, in score order.
    pub plans: Vec<StaffPlan>,
}

impl MeasureMetrics {
    pub fn offset_of(&self, onset: u32) -> f32 {
        self.columns
            .binary_search(&onset)
            .map(|i| self.offsets[i])
            .unwrap_or(0.0)
    }
}

/// Natural width for a time span: longer notes get more room, but less than proportionally.
fn spacing(ticks: u32) -> f32 {
    QUARTER_SPACING * (ticks as f32 / TICKS_PER_QUARTER as f32).powf(SPACING_EXPONENT)
}

pub(crate) fn measure_metrics(score: &Score, staves: &[&Staff], measure: usize) -> MeasureMetrics {
    let info = score.measures[measure];
    let capacity = info.capacity();
    // Only a pickup at the start of the score begins partway through the bar.
    let metric_offset = if measure == 0 {
        info.time_signature.ticks_per_measure() - capacity
    } else {
        0
    };
    let plans: Vec<StaffPlan> = staves
        .iter()
        .map(|staff| plan_staff(staff.clef, info, metric_offset, &staff.measures[measure]))
        .collect();

    let mut extents: BTreeMap<u32, (f32, f32)> = BTreeMap::new();
    for event in plans.iter().flat_map(|p| &p.voices).flat_map(|v| &v.events) {
        let entry = extents.entry(event.onset).or_insert((0.0, 0.0));
        entry.0 = entry.0.max(event.left);
        entry.1 = entry.1.max(event.right);
    }
    for staff in staves {
        for voice in &staff.measures[measure].voices {
            let mut onset = 0;
            for event in &voice.events {
                if event.markings.register.is_some() {
                    let (left, right) = emit::register_extent();
                    let entry = extents.entry(onset).or_insert((0.0, 0.0));
                    entry.0 = entry.0.max(left);
                    entry.1 = entry.1.max(right);
                }
                if let Some(dynamic) = event.markings.dynamic {
                    let (left, right) = volume::dynamic_extent(dynamic);
                    let entry = extents.entry(onset).or_insert((0.0, 0.0));
                    entry.0 = entry.0.max(left);
                    entry.1 = entry.1.max(right);
                }
                onset += event.duration.ticks();
            }
        }
    }
    for mark in score
        .tempo_directions
        .iter()
        .filter(|mark| mark.measure == measure)
    {
        let width = match mark.direction {
            notula_model::TempoDirection::Accelerando => 4.6,
            notula_model::TempoDirection::Ritardando => 2.8,
            notula_model::TempoDirection::Rallentando => 3.6,
            notula_model::TempoDirection::ATempo => 4.0,
            notula_model::TempoDirection::Rubato => 3.2,
        };
        let extent = &mut extents.entry(mark.tick).or_insert((0.0, 0.0)).1;
        *extent = extent.max(width);
    }
    for mark in score
        .tempo_changes
        .iter()
        .filter(|mark| mark.measure == measure && !mark.hidden)
    {
        let extent = &mut extents.entry(mark.tick).or_insert((0.0, 0.0)).1;
        *extent = extent.max(emit::metronome_width(mark.tempo));
    }

    let columns: Vec<u32> = extents.keys().copied().collect();
    let mut offsets = Vec::with_capacity(columns.len());
    let mut x = START_PAD + columns.first().map_or(0.0, |c| extents[c].0);
    for (i, tick) in columns.iter().enumerate() {
        offsets.push(x);
        let next = columns.get(i + 1);
        let until = next.copied().unwrap_or(capacity);
        let next_left = next.map_or(0.0, |n| extents[n].0);
        let needed = extents[tick].1 + MIN_CLEARANCE + next_left;
        x += spacing(until.saturating_sub(*tick)).max(needed);
    }

    // The final barline is drawn inside the measure's end; keep the usual room before it.
    if measure + 1 == score.measures.len() {
        x += FINAL_BARLINE_ROOM;
    }

    let body_width = if columns.is_empty() {
        spacing(capacity).max(MIN_BODY_WIDTH)
    } else {
        x.max(MIN_BODY_WIDTH)
    };

    MeasureMetrics {
        columns,
        offsets,
        body_width,
        plans,
    }
}
