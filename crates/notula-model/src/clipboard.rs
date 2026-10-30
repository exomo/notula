//! Copying a selection and pasting it elsewhere, overwriting what is there.
//!
//! A clip holds the selected events of each staff and voice at their offsets from the start of
//! the selection's grown span. Pasting replaces the clip's length of time in each target voice:
//! notes that would cross a barline are split into tied notes, gaps become rests, and slurs,
//! hairpins, and ties that lose a partner are removed.

use crate::command::Command;
use crate::edit;
use crate::error::ModelError;
use crate::history::Document;
use crate::ids::StaffId;
use crate::markings::{Markings, SlurPlacement};
use crate::rewrite::{
    NEW_EVENT, Placed, assign_ids, measure_of, measure_starts, rewrite_measures, split_at_barlines,
    voice_events,
};
use crate::score::{Event, EventKind, Score, Voice};
use crate::selection::Selection;

/// What was copied: per staff, the voices with their events.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Clip {
    /// Ticks from the start of the copied span to its end.
    pub length: u64,
    pub staves: Vec<ClipStaff>,
    /// The voice the copy was limited to; such a clip pastes into the cursor's voice.
    pub voice: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClipStaff {
    pub voices: Vec<ClipVoice>,
}

/// One voice of a copied staff. A voice without events still overwrites the target with rests.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClipVoice {
    pub voice: usize,
    /// Each event with its offset in ticks from the start of the clip.
    pub events: Vec<(u64, Event)>,
}

/// Start and stop indices of each paired mark, and the unpaired ones (`true` for a start).
type Pairing = (Vec<(usize, usize)>, Vec<(usize, bool)>);

/// Marks that run from one event to another in the same voice.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Spanner {
    Slur,
    PhraseSlur,
    Hairpin,
}

const SPANNERS: [Spanner; 3] = [Spanner::Slur, Spanner::PhraseSlur, Spanner::Hairpin];

impl Spanner {
    fn flags(self, markings: &Markings) -> (bool, bool) {
        match self {
            Spanner::Slur => (markings.slur_start, markings.slur_stop),
            Spanner::PhraseSlur => (markings.phrase_slur_start, markings.phrase_slur_stop),
            Spanner::Hairpin => (markings.hairpin_start.is_some(), markings.hairpin_stop),
        }
    }

    /// Removes the start of this mark, or its stop.
    fn clear(self, markings: &mut Markings, start: bool) {
        match (self, start) {
            (Spanner::Slur, true) => {
                markings.slur_start = false;
                markings.slur_placement = SlurPlacement::Auto;
            }
            (Spanner::Slur, false) => markings.slur_stop = false,
            (Spanner::PhraseSlur, true) => {
                markings.phrase_slur_start = false;
                markings.phrase_slur_placement = SlurPlacement::Auto;
            }
            (Spanner::PhraseSlur, false) => markings.phrase_slur_stop = false,
            (Spanner::Hairpin, true) => markings.hairpin_start = None,
            (Spanner::Hairpin, false) => markings.hairpin_stop = false,
        }
    }

    /// Start and stop indices of each mark in a run of events, and the starts (`true`) and
    /// stops without a partner, paired as `edit::slurs` and `edit::hairpins` read them.
    fn pairs(self, events: &[Placed]) -> Pairing {
        let mut pairs = Vec::new();
        let mut unpaired = Vec::new();
        let mut open: Option<usize> = None;
        for (index, item) in events.iter().enumerate() {
            let (starts, stops) = self.flags(&item.event.markings);
            if self == Spanner::Hairpin {
                // A stop closes the open hairpin before a start opens the next; a start while
                // one is open ends that one at the previous event.
                match open.take() {
                    Some(current) if stops => pairs.push((current, index)),
                    Some(current) if starts => pairs.push((current, index - 1)),
                    Some(current) => open = Some(current),
                    None if starts && stops => {
                        pairs.push((index, index));
                        continue;
                    }
                    None if stops => unpaired.push((index, false)),
                    None => {}
                }
                if starts && open.is_none() {
                    open = Some(index);
                }
            } else {
                if stops {
                    match open.take() {
                        Some(start) => pairs.push((start, index)),
                        None => unpaired.push((index, false)),
                    }
                }
                if starts {
                    if open.is_none() {
                        open = Some(index);
                    } else {
                        unpaired.push((index, true));
                    }
                }
            }
        }
        if let Some(start) = open {
            unpaired.push((start, true));
        }
        (pairs, unpaired)
    }
}

/// Copies the selection's grown span: the selected staves, the voices the filter allows that
/// exist there, and the events starting inside. Marks reaching outside the span lose that end,
/// and a tie out of the span is removed.
pub fn copy(score: &Score, selection: &Selection) -> Result<Clip, ModelError> {
    let resolved = selection.resolve(score).ok_or(ModelError::Invalid(
        "the selection no longer fits the score",
    ))?;
    let span = resolved.span;
    if span.end <= span.start {
        return Err(ModelError::Invalid("select notes to copy them"));
    }
    let starts = measure_starts(score);
    let (first, last) = (
        measure_of(&starts, span.start),
        measure_of(&starts, span.end - 1),
    );
    let mut staves = Vec::new();
    for &staff_id in &resolved.staves {
        let staff = score.staff(staff_id)?;
        let voice_count = staff.measures[first..=last]
            .iter()
            .map(|measure| measure.voices.len())
            .max()
            .unwrap_or(0);
        let mut voices = Vec::new();
        for voice in 0..voice_count {
            if selection.voice.is_some_and(|only| only != voice) {
                continue;
            }
            let mut events: Vec<Placed> = voice_events(staff, &starts, voice)
                .into_iter()
                .filter(|item| span.start <= item.start && item.start < span.end)
                .collect();
            for spanner in SPANNERS {
                for (index, start) in spanner.pairs(&events).1 {
                    spanner.clear(&mut events[index].event.markings, start);
                }
            }
            if let Some(last) = events.last_mut()
                && let EventKind::Chord { tied, .. } = &mut last.event.kind
            {
                *tied = false;
            }
            voices.push(ClipVoice {
                voice,
                events: events
                    .into_iter()
                    .map(|item| (item.start - span.start, item.event))
                    .collect(),
            });
        }
        staves.push(ClipStaff { voices });
    }
    Ok(Clip {
        length: span.end - span.start,
        staves,
        voice: selection.voice,
    })
}

/// Copies the selection, then turns its notes into rests, as one undo step.
pub fn cut(doc: &mut Document, selection: &Selection) -> Result<Clip, ModelError> {
    let clip = copy(doc.score(), selection)?;
    let events = selection
        .resolve(doc.score())
        .map(|resolved| resolved.events)
        .unwrap_or_default();
    edit::delete_events(doc, &events)?;
    Ok(clip)
}

/// Pastes at the cursor, or at the start of a range selection, and returns the pasted range
/// as a selection. A clip of one staff goes into every selected staff; a clip of several needs
/// exactly as many selected staves. Measures are added when the paste runs past the end.
pub fn paste(
    doc: &mut Document,
    selection: &Selection,
    clip: &Clip,
) -> Result<Selection, ModelError> {
    let resolved = selection.resolve(doc.score()).ok_or(ModelError::Invalid(
        "the selection no longer fits the score",
    ))?;
    let at = if selection.is_caret() {
        selection.focus
    } else {
        resolved.span.start
    };
    let targets: Vec<(StaffId, &ClipStaff)> = match clip.staves.len() {
        0 => return Err(ModelError::Invalid("nothing has been copied")),
        1 => resolved
            .staves
            .iter()
            .map(|staff| (*staff, &clip.staves[0]))
            .collect(),
        count if count == resolved.staves.len() => {
            resolved.staves.iter().copied().zip(&clip.staves).collect()
        }
        _ => {
            return Err(ModelError::Invalid(
                "span as many staves as were copied to paste them",
            ));
        }
    };
    let end = at + clip.length;

    doc.transaction(|doc| {
        while measure_starts(doc.score())
            .last()
            .is_some_and(|total| *total < end)
        {
            let index = doc.score().measures.len();
            let data = doc.score().blank_measure(index);
            doc.execute(Command::InsertMeasure { index, data })?;
        }
        let starts = measure_starts(doc.score());
        let measures = (measure_of(&starts, at), measure_of(&starts, end - 1));
        for &(staff, clip_staff) in &targets {
            for clip_voice in &clip_staff.voices {
                let voice = match clip.voice {
                    Some(_) => cursor_voice(doc.score(), selection, staff, measures.0),
                    None => clip_voice.voice,
                };
                if !ensure_voice(doc, staff, voice, measures, clip_voice.events.is_empty())? {
                    continue;
                }
                let mut commands =
                    paste_voice(doc.score(), &starts, staff, voice, (at, end), clip_voice)?;
                if commands.is_empty() {
                    continue;
                }
                assign_ids(doc, &mut commands);
                doc.execute(Command::Batch { commands })?;
            }
        }
        Ok(())
    })?;

    Ok(Selection {
        anchor: at,
        focus: end,
        voice: clip.voice.map(|_| selection.anchor_voice),
        ..*selection
    })
}

/// The voice a single-voice clip goes into: the cursor's voice, or in another staff the same
/// voice if that staff has it, otherwise the first.
fn cursor_voice(score: &Score, selection: &Selection, staff: StaffId, measure: usize) -> usize {
    let voices = score
        .staff(staff)
        .ok()
        .and_then(|data| data.measures.get(measure))
        .map_or(0, |content| content.voices.len());
    if staff == selection.anchor_staff || selection.anchor_voice < voices {
        selection.anchor_voice
    } else {
        0
    }
}

/// Makes sure `voice` exists in the measures pasted into, adding empty voices only to those
/// measures that lack it. Returns false when a clip voice without events would only add an
/// empty voice.
fn ensure_voice(
    doc: &mut Document,
    staff: StaffId,
    voice: usize,
    (first, last): (usize, usize),
    empty: bool,
) -> Result<bool, ModelError> {
    let counts: Vec<usize> = doc.score().staff(staff)?.measures[first..=last]
        .iter()
        .map(|content| content.voices.len())
        .collect();
    if counts.iter().all(|count| *count > voice) {
        return Ok(true);
    }
    if empty {
        return Ok(false);
    }
    for (measure, count) in (first..=last).zip(counts) {
        for _ in count..=voice {
            doc.execute(Command::PushVoice {
                staff,
                measure,
                voice: Voice::default(),
            })?;
        }
    }
    Ok(true)
}

/// Commands that write one clip voice over `at..end` of a target voice.
fn paste_voice(
    score: &Score,
    starts: &[u64],
    staff_id: StaffId,
    voice: usize,
    (at, end): (u64, u64),
    clip: &ClipVoice,
) -> Result<Vec<Command>, ModelError> {
    let staff = score.staff(staff_id)?;
    let mut old = voice_events(staff, starts, voice);
    let overlaps = |item: &Placed| item.start < end && at < item.end();
    // A note reaching into the pasted time from before keeps its beginning.
    let shortened = |item: &Placed| {
        item.start < at && at < item.end() && matches!(item.event.kind, EventKind::Chord { .. })
    };
    let gone: Vec<bool> = old
        .iter()
        .map(|item| overlaps(item) && !shortened(item))
        .collect();

    // Marks whose partner is overwritten lose their other end, and a mark around the pasted
    // time goes when the clip brings marks of the same kind, which it would otherwise enclose.
    for spanner in SPANNERS {
        let clip_has = clip.events.iter().any(|(_, event)| {
            let (starts, stops) = spanner.flags(&event.markings);
            starts || stops
        });
        for (start, stop) in spanner.pairs(&old).0 {
            let around = old[start].start < at && end <= old[stop].start;
            if gone[start] && !gone[stop] || around && clip_has {
                spanner.clear(&mut old[stop].event.markings, false);
            }
            if gone[stop] && !gone[start] || around && clip_has {
                spanner.clear(&mut old[start].event.markings, true);
            }
        }
    }
    // Ties into overwritten time are removed.
    for (index, item) in old.iter_mut().enumerate() {
        let breaks = shortened(item) || gone.get(index + 1).copied().unwrap_or(false);
        if breaks && let EventKind::Chord { tied, .. } = &mut item.event.kind {
            *tied = false;
        }
    }

    let mut items = Vec::new();
    for (item, gone) in old.into_iter().zip(gone) {
        if gone {
            continue;
        }
        if shortened(&item) {
            let start = item.start;
            items.extend(split_at_barlines(item.event, start, at, starts)?);
        } else {
            items.push(item);
        }
    }
    for (offset, event) in &clip.events {
        if matches!(event.kind, EventKind::Rest) && event.markings.is_empty() {
            continue;
        }
        let start = at + offset;
        let length = u64::from(event.duration.ticks());
        let event = Event {
            id: NEW_EVENT,
            ..event.clone()
        };
        items.extend(split_at_barlines(event, start, start + length, starts)?);
    }
    items.sort_by_key(|item| item.start);
    rewrite_measures(staff, starts, voice, 0..=score.measures.len() - 1, &items)
}
