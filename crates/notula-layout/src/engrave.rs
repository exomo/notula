//! Per-staff planning of one measure: stem directions, beams, accidentals and notehead placement.
//! Horizontal positions are decided later, so everything here is relative to the column origin.

use std::collections::HashMap;

use notula_model::{
    Articulation, Clef, EventId, EventKind, MeasureInfo, NoteValue, Pitch, SlurPlacement,
    StaffMeasure, Step, TICKS_PER_QUARTER, TimeSignature,
};

use crate::smufl;
use crate::staff::{key_alter, pitch_y};

const MIDDLE_LINE: f32 = 2.0;
const ACCIDENTAL_MARGIN: f32 = 0.3;
const ACCIDENTAL_SLOT_GAP: f32 = 0.15;
/// Accidentals closer than a seventh would collide, so they get separate columns.
const ACCIDENTAL_STACK_DISTANCE: f32 = 3.0;
const DOT_GAP: f32 = 0.4;
const DOT_ADVANCE: f32 = 0.6;

pub(crate) struct AccidentalPlan {
    pub glyph: char,
    /// Offset of the glyph origin from the column, negative.
    pub dx: f32,
}

pub(crate) struct NotePlan {
    pub pitch: Pitch,
    pub y: f32,
    /// Moved to the other side of the stem because a neighbour a second away is on this side.
    pub displaced: bool,
    pub accidental: Option<AccidentalPlan>,
}

pub(crate) enum Body {
    Rest { y: f32 },
    Chord { notes: Vec<NotePlan> },
}

pub(crate) struct EventPlan {
    pub id: EventId,
    pub onset: u32,
    pub value: NoteValue,
    pub dots: u8,
    pub articulations: Vec<Articulation>,
    pub tied: bool,
    pub slur_start: bool,
    pub slur_stop: bool,
    pub slur_placement: SlurPlacement,
    pub phrase_slur_start: bool,
    pub phrase_slur_stop: bool,
    pub phrase_slur_placement: SlurPlacement,
    pub stem_up: bool,
    /// Another voice sounds at the same time, so the stem points away from it and marks go on
    /// the stem side.
    pub shares_staff: bool,
    pub body: Body,
    /// Space needed left of the column.
    pub left: f32,
    /// Space needed right of the column, including dots.
    pub right: f32,
    /// Offset of the first augmentation dot from the column.
    pub dot_x: f32,
}

#[derive(Default)]
pub(crate) struct VoicePlan {
    pub events: Vec<EventPlan>,
    /// Indices into `events`; each group has at least two chords.
    pub beams: Vec<Vec<usize>>,
}

pub(crate) struct StaffPlan {
    pub voices: Vec<VoicePlan>,
}

impl StaffPlan {
    pub fn is_empty(&self) -> bool {
        self.voices.iter().all(|voice| voice.events.is_empty())
    }
}

struct DraftNote {
    pitch: Pitch,
    y: f32,
    diatonic: i16,
    accidental: Option<i8>,
}

struct Draft {
    id: EventId,
    onset: u32,
    value: NoteValue,
    dots: u8,
    articulations: Vec<Articulation>,
    tied: bool,
    slur_start: bool,
    slur_stop: bool,
    slur_placement: SlurPlacement,
    phrase_slur_start: bool,
    phrase_slur_stop: bool,
    phrase_slur_placement: SlurPlacement,
    /// Empty for rests; otherwise sorted from the lowest to the highest pitch.
    notes: Vec<DraftNote>,
}

pub(crate) fn dots_extent(dots: u8) -> f32 {
    if dots == 0 {
        0.0
    } else {
        DOT_GAP + DOT_ADVANCE * f32::from(dots - 1) + smufl::DOT_WIDTH
    }
}

pub(crate) fn dot_advance() -> f32 {
    DOT_ADVANCE
}

/// `metric_offset` is how far into the bar the measure starts, which is non-zero for a pickup.
pub(crate) fn plan_staff(
    clef: Clef,
    info: MeasureInfo,
    metric_offset: u32,
    content: &StaffMeasure,
) -> StaffPlan {
    let mut drafts: Vec<Vec<Draft>> = content
        .voices
        .iter()
        .map(|voice| {
            let mut onset = 0;
            voice
                .events
                .iter()
                .map(|event| {
                    let notes = match &event.kind {
                        EventKind::Rest => Vec::new(),
                        EventKind::Chord { pitches, .. } => {
                            let mut notes: Vec<_> = pitches
                                .iter()
                                .map(|&pitch| DraftNote {
                                    pitch,
                                    y: pitch_y(pitch, clef),
                                    diatonic: pitch.diatonic_index(),
                                    accidental: None,
                                })
                                .collect();
                            notes.sort_by_key(|note| note.diatonic);
                            notes
                        }
                    };
                    let draft = Draft {
                        id: event.id,
                        onset,
                        value: event.duration.value,
                        dots: event.duration.dots,
                        articulations: event.markings.articulations.clone(),
                        tied: matches!(event.kind, EventKind::Chord { tied: true, .. }),
                        slur_start: event.markings.slur_start,
                        slur_stop: event.markings.slur_stop,
                        slur_placement: event.markings.slur_placement,
                        phrase_slur_start: event.markings.phrase_slur_start,
                        phrase_slur_stop: event.markings.phrase_slur_stop,
                        phrase_slur_placement: event.markings.phrase_slur_placement,
                        notes,
                    };
                    onset += event.duration.ticks();
                    draft
                })
                .collect()
        })
        .collect();

    assign_accidentals(&mut drafts, content, info);

    let active_ends: Vec<_> = content
        .voices
        .iter()
        .map(|voice| voice.end_tick.unwrap_or_else(|| info.capacity()))
        .collect();
    let voices = drafts
        .iter()
        .enumerate()
        .map(|(voice_index, voice_drafts)| {
            let forced: Vec<_> = voice_drafts
                .iter()
                .map(|draft| {
                    let active: Vec<_> = active_ends
                        .iter()
                        .enumerate()
                        .filter_map(|(index, &end)| (draft.onset < end).then_some(index))
                        .collect();
                    (active.len() > 1).then(|| {
                        active
                            .iter()
                            .position(|&index| index == voice_index)
                            .unwrap_or(voice_index)
                            % 2
                            == 0
                    })
                })
                .collect();
            plan_voice(voice_drafts, info.time_signature, metric_offset, &forced)
        })
        .collect();

    StaffPlan { voices }
}

/// Accidentals follow the key signature and are remembered per pitch until the barline.
fn assign_accidentals(drafts: &mut [Vec<Draft>], content: &StaffMeasure, info: MeasureInfo) {
    let mut order: Vec<(u32, usize, usize)> = drafts
        .iter()
        .enumerate()
        .flat_map(|(v, voice)| {
            voice
                .iter()
                .enumerate()
                .map(move |(e, draft)| (draft.onset, v, e))
        })
        .collect();
    order.sort_unstable();

    let mut memory: HashMap<(Step, i8), i8> = HashMap::new();
    for (_, v, e) in order {
        let EventKind::Chord { pitches, .. } = &content.voices[v].events[e].kind else {
            continue;
        };
        for pitch in pitches {
            let current = memory
                .get(&(pitch.step, pitch.octave))
                .copied()
                .unwrap_or_else(|| key_alter(info.key_signature.fifths, pitch.step));
            if pitch.alter != current {
                let diatonic = pitch.diatonic_index();
                if let Some(note) = drafts[v][e]
                    .notes
                    .iter_mut()
                    .find(|n| n.diatonic == diatonic)
                {
                    note.accidental = Some(pitch.alter);
                }
            }
            memory.insert((pitch.step, pitch.octave), pitch.alter);
        }
    }
}

fn beat_ticks(time: TimeSignature) -> u32 {
    let unit = 4 * TICKS_PER_QUARTER / u32::from(time.denominator);
    if time.denominator < 8 {
        unit
    } else if time.numerator.is_multiple_of(3) {
        3 * unit
    } else {
        2 * unit
    }
}

/// Signed distance of the note farthest from the middle line; positive means below it.
fn extreme_offset(notes: &[DraftNote]) -> f32 {
    let top = notes.iter().map(|n| n.y).fold(f32::INFINITY, f32::min);
    let bottom = notes.iter().map(|n| n.y).fold(f32::NEG_INFINITY, f32::max);
    if bottom - MIDDLE_LINE > MIDDLE_LINE - top {
        bottom - MIDDLE_LINE
    } else {
        top - MIDDLE_LINE
    }
}

fn plan_voice(
    drafts: &[Draft],
    time: TimeSignature,
    metric_offset: u32,
    forced_up: &[Option<bool>],
) -> VoicePlan {
    let beat = beat_ticks(time);
    let mut groups: Vec<Vec<usize>> = Vec::new();
    let mut current: Vec<usize> = Vec::new();
    let mut flush = |current: &mut Vec<usize>| {
        if current.len() >= 2 {
            groups.push(std::mem::take(current));
        } else {
            current.clear();
        }
    };
    for (i, draft) in drafts.iter().enumerate() {
        if draft.notes.is_empty() || smufl::flag_count(draft.value) == 0 {
            flush(&mut current);
            continue;
        }
        if let Some(&first) = current.first()
            && ((drafts[first].onset + metric_offset) / beat
                != (draft.onset + metric_offset) / beat
                || forced_up[first] != forced_up[i])
        {
            flush(&mut current);
        }
        current.push(i);
    }
    flush(&mut current);

    let mut stem_up: Vec<bool> = drafts
        .iter()
        .enumerate()
        .map(|(index, draft)| {
            forced_up[index]
                .unwrap_or_else(|| !draft.notes.is_empty() && extreme_offset(&draft.notes) > 0.0)
        })
        .collect();
    for group in &groups {
        if group.iter().all(|&index| forced_up[index].is_none()) {
            let sum: f32 = group
                .iter()
                .map(|&i| extreme_offset(&drafts[i].notes))
                .sum();
            for &i in group {
                stem_up[i] = sum > 0.0;
            }
        }
    }

    VoicePlan {
        events: drafts
            .iter()
            .zip(stem_up)
            .zip(forced_up)
            .map(|((draft, up), forced)| finish_event(draft, up, forced.is_some()))
            .collect(),
        beams: groups,
    }
}

fn finish_event(draft: &Draft, stem_up: bool, shares_staff: bool) -> EventPlan {
    if draft.notes.is_empty() {
        let (_, width, y) = smufl::rest(draft.value);
        return EventPlan {
            id: draft.id,
            onset: draft.onset,
            value: draft.value,
            dots: draft.dots,
            articulations: draft.articulations.clone(),
            tied: draft.tied,
            slur_start: draft.slur_start,
            slur_stop: draft.slur_stop,
            slur_placement: draft.slur_placement,
            phrase_slur_start: draft.phrase_slur_start,
            phrase_slur_stop: draft.phrase_slur_stop,
            phrase_slur_placement: draft.phrase_slur_placement,
            stem_up,
            shares_staff,
            body: Body::Rest { y },
            left: 0.0,
            right: width + dots_extent(draft.dots),
            dot_x: width + DOT_GAP,
        };
    }

    let head_width = smufl::notehead(draft.value).1;
    let count = draft.notes.len();
    let mut displaced = vec![false; count];
    if stem_up {
        for i in 1..count {
            displaced[i] =
                draft.notes[i].diatonic - draft.notes[i - 1].diatonic == 1 && !displaced[i - 1];
        }
    } else {
        for i in (0..count - 1).rev() {
            displaced[i] =
                draft.notes[i + 1].diatonic - draft.notes[i].diatonic == 1 && !displaced[i + 1];
        }
    }
    let any_displaced = displaced.iter().any(|&d| d);
    let base_left = if any_displaced && !stem_up {
        head_width
    } else {
        0.0
    };
    let heads_right = if any_displaced && stem_up {
        2.0 * head_width
    } else {
        head_width
    };

    // Highest note first, each in the first column where it clears the previous one.
    struct Slot {
        last_y: f32,
        width: f32,
    }
    let mut slots: Vec<Slot> = Vec::new();
    let mut slot_of: Vec<Option<usize>> = vec![None; count];
    for i in (0..count).rev() {
        let Some(alter) = draft.notes[i].accidental else {
            continue;
        };
        let (_, width) = smufl::accidental(alter);
        let y = draft.notes[i].y;
        let slot = slots
            .iter()
            .position(|s| y - s.last_y >= ACCIDENTAL_STACK_DISTANCE)
            .unwrap_or_else(|| {
                slots.push(Slot {
                    last_y: f32::NEG_INFINITY,
                    width: 0.0,
                });
                slots.len() - 1
            });
        slots[slot].last_y = y;
        slots[slot].width = slots[slot].width.max(width);
        slot_of[i] = Some(slot);
    }
    let mut slot_edge = Vec::with_capacity(slots.len());
    let mut edge = base_left + ACCIDENTAL_MARGIN;
    for slot in &slots {
        slot_edge.push(edge);
        edge += slot.width + ACCIDENTAL_SLOT_GAP;
    }
    let left = match slots.last() {
        Some(last) => slot_edge[slots.len() - 1] + last.width,
        None => base_left,
    };

    let notes = draft
        .notes
        .iter()
        .enumerate()
        .map(|(i, note)| NotePlan {
            pitch: note.pitch,
            y: note.y,
            displaced: displaced[i],
            accidental: note.accidental.zip(slot_of[i]).map(|(alter, slot)| {
                let (glyph, width) = smufl::accidental(alter);
                AccidentalPlan {
                    glyph,
                    dx: -(slot_edge[slot] + width),
                }
            }),
        })
        .collect();

    EventPlan {
        id: draft.id,
        onset: draft.onset,
        value: draft.value,
        dots: draft.dots,
        articulations: draft.articulations.clone(),
        tied: draft.tied,
        slur_start: draft.slur_start,
        slur_stop: draft.slur_stop,
        slur_placement: draft.slur_placement,
        phrase_slur_start: draft.phrase_slur_start,
        phrase_slur_stop: draft.phrase_slur_stop,
        phrase_slur_placement: draft.phrase_slur_placement,
        stem_up,
        shares_staff,
        body: Body::Chord { notes },
        left,
        right: heads_right + dots_extent(draft.dots),
        dot_x: heads_right + DOT_GAP,
    }
}

#[cfg(test)]
mod tests {
    use notula_model::{Articulation, Duration, Event, KeySignature, Pitch, Voice};

    use super::*;

    fn plan_staff(clef: Clef, info: MeasureInfo, content: &StaffMeasure) -> StaffPlan {
        super::plan_staff(clef, info, 0, content)
    }

    fn chord(id: u64, value: NoteValue, pitches: &[Pitch]) -> Event {
        Event::new(
            EventId(id),
            Duration::new(value, 0),
            EventKind::Chord {
                pitches: pitches.to_vec(),
                tied: false,
            },
        )
    }

    fn measure(events: Vec<Event>) -> StaffMeasure {
        StaffMeasure {
            voices: vec![Voice {
                events,
                ..Voice::default()
            }],
        }
    }

    fn c_major_44() -> MeasureInfo {
        MeasureInfo::default()
    }

    fn accidental_glyph(plan: &StaffPlan, event: usize, note: usize) -> Option<char> {
        match &plan.voices[0].events[event].body {
            Body::Chord { notes } => notes[note].accidental.as_ref().map(|a| a.glyph),
            Body::Rest { .. } => None,
        }
    }

    #[test]
    fn accidentals_are_remembered_within_the_measure() {
        let f_sharp = Pitch::new(Step::F, 1, 4);
        let f_natural = Pitch::new(Step::F, 0, 4);
        let plan = plan_staff(
            Clef::Treble,
            c_major_44(),
            &measure(vec![
                chord(1, NoteValue::Quarter, &[f_sharp]),
                chord(2, NoteValue::Quarter, &[f_sharp]),
                chord(3, NoteValue::Quarter, &[f_natural]),
                chord(4, NoteValue::Quarter, &[f_natural]),
            ]),
        );
        assert_eq!(accidental_glyph(&plan, 0, 0), Some('\u{E262}'));
        assert_eq!(accidental_glyph(&plan, 1, 0), None);
        assert_eq!(accidental_glyph(&plan, 2, 0), Some('\u{E261}'));
        assert_eq!(accidental_glyph(&plan, 3, 0), None);
    }

    #[test]
    fn accentuation_is_carried_into_the_event_plan() {
        let mut event = chord(1, NoteValue::Quarter, &[Pitch::new(Step::C, 0, 4)]);
        event.markings.articulations.push(Articulation::Accent);
        let plan = plan_staff(Clef::Treble, c_major_44(), &measure(vec![event]));

        assert_eq!(
            plan.voices[0].events[0].articulations,
            vec![Articulation::Accent]
        );
        let mut canvas = crate::render::Canvas::default();
        crate::emit::emit_voice(&mut canvas, &plan.voices[0], 0.0, &|_| 0.0);

        assert!(canvas.items.iter().any(|item| matches!(
            item.shape,
            crate::render::Shape::Glyph {
                glyph: '\u{E4A1}',
                ..
            }
        )));
    }

    #[test]
    fn standard_articulations_render_and_fermatas_are_deferred() {
        let mut note = chord(1, NoteValue::Quarter, &[Pitch::new(Step::C, 0, 4)]);
        note.markings.articulations = vec![
            Articulation::Staccato,
            Articulation::Staccatissimo,
            Articulation::Tenuto,
            Articulation::Accent,
            Articulation::Marcato,
            Articulation::Fermata,
        ];
        let mut rest = Event::new(
            EventId(2),
            Duration::new(NoteValue::Quarter, 0),
            EventKind::Rest,
        );
        rest.markings.articulations.push(Articulation::Fermata);
        let plan = plan_staff(Clef::Treble, c_major_44(), &measure(vec![note, rest]));
        let mut canvas = crate::render::Canvas::default();
        crate::emit::emit_voice(&mut canvas, &plan.voices[0], 0.0, &|_| 0.0);

        let glyphs: Vec<_> = canvas
            .items
            .iter()
            .filter_map(|item| match item.shape {
                crate::render::Shape::Glyph { glyph, .. } => Some((item.event, glyph)),
                _ => None,
            })
            .collect();
        for glyph in ['\u{E4A3}', '\u{E4A7}', '\u{E4A5}', '\u{E4A1}', '\u{E4AD}'] {
            assert!(glyphs.contains(&(Some(EventId(1)), glyph)));
        }
        assert!(!glyphs.iter().any(|(_, glyph)| *glyph == '\u{E4C0}'));
        let fermatas = crate::emit::fermatas(&plan.voices, 0.0, &|onset| onset as f32);
        assert_eq!(
            fermatas
                .iter()
                .map(|(event, _, _, _)| *event)
                .collect::<Vec<_>>(),
            vec![EventId(1), EventId(2)]
        );
    }

    #[test]
    fn articulation_is_centered_on_displaced_chord_and_avoids_staff_lines() {
        let mut event = chord(
            1,
            NoteValue::Quarter,
            &[Pitch::new(Step::C, 0, 4), Pitch::new(Step::D, 0, 4)],
        );
        event.markings.articulations.push(Articulation::Staccato);
        let plan = plan_staff(Clef::Treble, c_major_44(), &measure(vec![event]));
        let event_plan = &plan.voices[0].events[0];
        let mut canvas = crate::render::Canvas::default();
        crate::emit::emit_voice(&mut canvas, &plan.voices[0], 0.0, &|_| 0.0);

        let articulation = canvas.items.iter().find_map(|item| match item.shape {
            crate::render::Shape::Glyph {
                x,
                y,
                glyph: '\u{E4A3}',
                centered,
                ..
            } => Some((x, y, centered)),
            _ => None,
        });
        let (x, y, centered) = articulation.expect("staccato glyph should be emitted");
        let head_width = crate::smufl::notehead(NoteValue::Quarter).1;
        let expected_x = head_width - crate::smufl::STEM_THICKNESS / 2.0;

        assert!(event_plan.stem_up);
        assert!(centered);
        assert!((x - expected_x).abs() < 0.001);
        assert!((y * 2.0).fract().abs() < 0.001);
        assert!((y.fract().abs() - 0.5).abs() < 0.001);
    }

    #[test]
    fn staccato_and_tenuto_keep_clear_vertical_separation() {
        let mut event = chord(1, NoteValue::Quarter, &[Pitch::new(Step::C, 0, 4)]);
        event.markings.articulations = vec![Articulation::Staccato, Articulation::Tenuto];
        let plan = plan_staff(Clef::Treble, c_major_44(), &measure(vec![event]));
        let mut canvas = crate::render::Canvas::default();
        crate::emit::emit_voice(&mut canvas, &plan.voices[0], 0.0, &|_| 0.0);

        let baselines: Vec<_> = canvas
            .items
            .iter()
            .filter_map(|item| match item.shape {
                crate::render::Shape::Glyph {
                    y,
                    glyph: '\u{E4A3}' | '\u{E4A5}',
                    ..
                } => Some(y),
                _ => None,
            })
            .collect();

        assert_eq!(baselines.len(), 2);
        assert!((baselines[0] - baselines[1]).abs() >= 1.0);
    }

    #[test]
    fn key_signature_suppresses_accidentals() {
        let info = MeasureInfo {
            key_signature: KeySignature { fifths: 1 },
            ..MeasureInfo::default()
        };
        let plan = plan_staff(
            Clef::Treble,
            info,
            &measure(vec![
                chord(1, NoteValue::Quarter, &[Pitch::new(Step::F, 1, 4)]),
                chord(2, NoteValue::Quarter, &[Pitch::new(Step::F, 0, 5)]),
            ]),
        );
        assert_eq!(accidental_glyph(&plan, 0, 0), None);
        assert_eq!(accidental_glyph(&plan, 1, 0), Some('\u{E261}'));
    }

    #[test]
    fn octaves_do_not_share_accidentals() {
        let plan = plan_staff(
            Clef::Treble,
            c_major_44(),
            &measure(vec![
                chord(1, NoteValue::Quarter, &[Pitch::new(Step::F, 1, 4)]),
                chord(2, NoteValue::Quarter, &[Pitch::new(Step::F, 0, 5)]),
            ]),
        );
        assert_eq!(accidental_glyph(&plan, 1, 0), None);
    }

    #[test]
    fn eighths_beam_per_beat() {
        let c4 = Pitch::new(Step::C, 0, 4);
        let events = (1..=8)
            .map(|id| chord(id, NoteValue::Eighth, &[c4]))
            .collect();
        let plan = plan_staff(Clef::Treble, c_major_44(), &measure(events));
        assert_eq!(
            plan.voices[0].beams,
            vec![vec![0, 1], vec![2, 3], vec![4, 5], vec![6, 7]]
        );
    }

    #[test]
    fn compound_meter_beams_by_dotted_quarter() {
        let c4 = Pitch::new(Step::C, 0, 4);
        let events = (1..=6)
            .map(|id| chord(id, NoteValue::Eighth, &[c4]))
            .collect();
        let info = MeasureInfo {
            time_signature: TimeSignature {
                numerator: 6,
                denominator: 8,
            },
            ..MeasureInfo::default()
        };
        let plan = plan_staff(Clef::Treble, info, &measure(events));
        assert_eq!(plan.voices[0].beams, vec![vec![0, 1, 2], vec![3, 4, 5]]);
    }

    #[test]
    fn pickup_beams_follow_the_beats_of_the_whole_bar() {
        let c4 = Pitch::new(Step::C, 0, 4);
        let events = (1..=3)
            .map(|id| chord(id, NoteValue::Eighth, &[c4]))
            .collect();
        // Three eighths ending the bar start 2400 ticks in, so only the last two share a beat.
        let plan = super::plan_staff(Clef::Treble, c_major_44(), 2400, &measure(events));
        assert_eq!(plan.voices[0].beams, vec![vec![1, 2]]);
    }

    #[test]
    fn rests_and_long_notes_break_beams() {
        let c4 = Pitch::new(Step::C, 0, 4);
        let rest = Event::new(
            EventId(2),
            Duration::new(NoteValue::Eighth, 0),
            EventKind::Rest,
        );
        let plan = plan_staff(
            Clef::Treble,
            c_major_44(),
            &measure(vec![
                chord(1, NoteValue::Eighth, &[c4]),
                rest,
                chord(3, NoteValue::Eighth, &[c4]),
                chord(4, NoteValue::Eighth, &[c4]),
            ]),
        );
        assert_eq!(plan.voices[0].beams, vec![vec![2, 3]]);
    }

    #[test]
    fn stem_direction_follows_the_farthest_note() {
        let low = chord(1, NoteValue::Quarter, &[Pitch::new(Step::C, 0, 4)]);
        let high = chord(2, NoteValue::Quarter, &[Pitch::new(Step::A, 0, 5)]);
        let plan = plan_staff(Clef::Treble, c_major_44(), &measure(vec![low, high]));
        assert!(plan.voices[0].events[0].stem_up);
        assert!(!plan.voices[0].events[1].stem_up);
    }

    #[test]
    fn stem_direction_returns_to_single_voice_after_an_additional_voice_ends() {
        let high = chord(1, NoteValue::Quarter, &[Pitch::new(Step::A, 0, 5)]);
        let after_end = chord(3, NoteValue::Quarter, &[Pitch::new(Step::A, 0, 5)]);
        let two_voice_plan = plan_staff(
            Clef::Treble,
            c_major_44(),
            &StaffMeasure {
                voices: vec![
                    Voice {
                        events: vec![
                            high.clone(),
                            chord(4, NoteValue::Quarter, &[Pitch::new(Step::A, 0, 5)]),
                            after_end,
                        ],
                        ..Voice::default()
                    },
                    Voice {
                        events: vec![chord(2, NoteValue::Quarter, &[Pitch::new(Step::C, 0, 4)])],
                        end_tick: Some(1920),
                    },
                ],
            },
        );

        assert!(two_voice_plan.voices[0].events[0].stem_up);
        assert!(two_voice_plan.voices[0].events[1].stem_up);
        assert!(!two_voice_plan.voices[0].events[2].stem_up);
    }

    #[test]
    fn seconds_displace_the_upper_note_for_stem_up() {
        let plan = plan_staff(
            Clef::Treble,
            c_major_44(),
            &measure(vec![chord(
                1,
                NoteValue::Quarter,
                &[Pitch::new(Step::C, 0, 4), Pitch::new(Step::D, 0, 4)],
            )]),
        );
        let event = &plan.voices[0].events[0];
        assert!(event.stem_up);
        let Body::Chord { notes } = &event.body else {
            panic!("expected a chord");
        };
        assert!(!notes[0].displaced);
        assert!(notes[1].displaced);
    }

    #[test]
    fn chord_accidentals_stack_only_when_close() {
        let close = [Pitch::new(Step::C, 1, 4), Pitch::new(Step::E, 1, 4)];
        let plan = plan_staff(
            Clef::Treble,
            c_major_44(),
            &measure(vec![chord(1, NoteValue::Quarter, &close)]),
        );
        let Body::Chord { notes } = &plan.voices[0].events[0].body else {
            panic!("expected a chord");
        };
        let dx: Vec<f32> = notes
            .iter()
            .map(|n| n.accidental.as_ref().unwrap().dx)
            .collect();
        assert_ne!(dx[0], dx[1]);

        let far = [Pitch::new(Step::C, 1, 4), Pitch::new(Step::C, 1, 5)];
        let plan = plan_staff(
            Clef::Treble,
            c_major_44(),
            &measure(vec![chord(1, NoteValue::Quarter, &far)]),
        );
        let Body::Chord { notes } = &plan.voices[0].events[0].body else {
            panic!("expected a chord");
        };
        let dx: Vec<f32> = notes
            .iter()
            .map(|n| n.accidental.as_ref().unwrap().dx)
            .collect();
        assert_eq!(dx[0], dx[1]);
    }
}
