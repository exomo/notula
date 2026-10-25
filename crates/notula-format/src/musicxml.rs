//! MusicXML import and export.

use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt::Write;
use std::fs::File;
use std::io::{Read, Seek};
use std::path::Path;

use notula_model::{
    Articulation, Clef, Command, Document, Duration, Dynamic, Event, EventKind, Hairpin,
    KeySignature, Markings, MeasureInfo, NoteValue, Pitch, Score, Step, TICKS_PER_QUARTER,
    TimeSignature, TimeSignatureDisplay, Voice, VoiceLocation,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MusicXmlExport {
    pub xml: String,
    pub unsupported: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MusicXmlImport {
    pub score: Score,
    pub unsupported: Vec<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum MusicXmlError {
    #[error("invalid MusicXML: {0}")]
    Xml(#[from] roxmltree::Error),
    #[error("unsupported MusicXML document: {0}")]
    Unsupported(&'static str),
    #[error("invalid score data: {0}")]
    Model(#[from] notula_model::ModelError),
    #[error("MusicXML file I/O failed: {0}")]
    Io(#[from] std::io::Error),
}

pub fn export(score: &Score) -> MusicXmlExport {
    let mut xml = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"no\"?>\n<score-partwise version=\"4.0\">\n",
    );
    if !score.title.is_empty() {
        let _ = write!(
            xml,
            "  <work><work-title>{}</work-title></work>\n",
            escape(&score.title)
        );
    }
    if !score.composer.is_empty() {
        let _ = write!(
            xml,
            "  <identification><creator type=\"composer\">{}</creator></identification>\n",
            escape(&score.composer)
        );
    }
    xml.push_str("  <part-list>\n");
    for (part_index, part) in score.parts.iter().enumerate() {
        let id = part_id(part_index);
        let _ = write!(
            xml,
            "    <score-part id=\"{id}\"><part-name>{}</part-name>",
            escape(&part.name)
        );
        if !part.short_name.is_empty() {
            let _ = write!(
                xml,
                "<part-abbreviation>{}</part-abbreviation>",
                escape(&part.short_name)
            );
        }
        xml.push_str("</score-part>\n");
    }
    xml.push_str("  </part-list>\n");

    for (part_index, part) in score.parts.iter().enumerate() {
        let id = part_id(part_index);
        let _ = writeln!(xml, "  <part id=\"{id}\">");
        for (measure_index, info) in score.measures.iter().enumerate() {
            let implicit = if info.length.is_some() {
                " implicit=\"yes\""
            } else {
                ""
            };
            let _ = writeln!(
                xml,
                "    <measure number=\"{}\"{implicit}>",
                measure_index + 1
            );
            let _ = writeln!(
                xml,
                "      <attributes><divisions>{TICKS_PER_QUARTER}</divisions>"
            );
            let _ = writeln!(
                xml,
                "        <key><fifths>{}</fifths></key>",
                info.key_signature.fifths
            );
            let time_symbol = match info.time_signature_display {
                TimeSignatureDisplay::Numeric => "",
                TimeSignatureDisplay::Common => " symbol=\"common\"",
                TimeSignatureDisplay::AllaBreve => " symbol=\"cut\"",
            };
            let _ = writeln!(
                xml,
                "        <time{time_symbol}><beats>{}</beats><beat-type>{}</beat-type></time>",
                info.time_signature.numerator, info.time_signature.denominator
            );
            if part.staves.len() > 1 {
                let _ = writeln!(xml, "        <staves>{}</staves>", part.staves.len());
            }
            for (staff_index, staff) in part.staves.iter().enumerate() {
                let (sign, line) = clef(staff.clef);
                let _ = writeln!(
                    xml,
                    "        <clef number=\"{}\"><sign>{sign}</sign><line>{line}</line></clef>",
                    staff_index + 1
                );
            }
            xml.push_str("      </attributes>\n");

            let mut first_voice = true;
            for (staff_index, staff) in part.staves.iter().enumerate() {
                let content = &staff.measures[measure_index];
                for (voice_index, voice) in content.voices.iter().enumerate() {
                    if !first_voice {
                        let _ = writeln!(
                            xml,
                            "      <backup><duration>{}</duration></backup>",
                            info.capacity()
                        );
                    }
                    first_voice = false;
                    let mut onset = 0;
                    for event in &voice.events {
                        write_directions(&mut xml, event, staff_index + 1);
                        write_event(
                            &mut xml,
                            event,
                            voice_index + 1,
                            staff_index + 1,
                            info.key_signature.fifths,
                        );
                        onset += event.duration.ticks();
                    }
                    if onset < info.capacity() {
                        let _ = writeln!(
                            xml,
                            "      <forward><duration>{}</duration><voice>{}</voice><staff>{}</staff></forward>",
                            info.capacity() - onset,
                            voice_index + 1,
                            staff_index + 1
                        );
                    }
                }
            }
            xml.push_str("    </measure>\n");
        }
        xml.push_str("  </part>\n");
    }
    xml.push_str("</score-partwise>\n");

    MusicXmlExport {
        xml,
        unsupported: Vec::new(),
    }
}

pub fn export_file(score: &Score, path: &Path) -> Result<Vec<String>, MusicXmlError> {
    let result = export(score);
    let mut temp = path.as_os_str().to_owned();
    temp.push(".tmp");
    let temp = std::path::PathBuf::from(temp);
    let write_result = (|| -> Result<(), std::io::Error> {
        let mut file = File::create(&temp)?;
        use std::io::Write as _;
        file.write_all(result.xml.as_bytes())?;
        file.sync_all()?;
        std::fs::rename(&temp, path)
    })();
    if let Err(error) = write_result {
        let _ = std::fs::remove_file(&temp);
        return Err(MusicXmlError::Io(error));
    }
    Ok(result.unsupported)
}

pub fn import_file(path: &Path) -> Result<MusicXmlImport, MusicXmlError> {
    match path.extension().and_then(|extension| extension.to_str()) {
        Some(extension) if extension.eq_ignore_ascii_case("mxl") => {
            let file = File::open(path)?;
            import_mxl(file)
        }
        _ => {
            let xml = std::fs::read_to_string(path)?;
            import(&xml)
        }
    }
}

fn import_mxl<R: Read + Seek>(reader: R) -> Result<MusicXmlImport, MusicXmlError> {
    const MAX_XML_BYTES: u64 = 64 * 1024 * 1024;
    let mut archive = zip::ZipArchive::new(reader)
        .map_err(|_| MusicXmlError::Unsupported("invalid compressed MusicXML archive"))?;
    let container = archive
        .by_name("META-INF/container.xml")
        .map_err(|_| MusicXmlError::Unsupported("compressed MusicXML has no container.xml"))?;
    if container.size() > 1024 * 1024 {
        return Err(MusicXmlError::Unsupported(
            "compressed MusicXML container is too large",
        ));
    }
    let mut container_xml = String::new();
    container
        .take(1024 * 1024)
        .read_to_string(&mut container_xml)
        .map_err(|_| MusicXmlError::Unsupported("could not read compressed MusicXML container"))?;
    let sanitized_container = without_doctype(&container_xml);
    let container_doc = roxmltree::Document::parse(&sanitized_container)?;
    let rootfile = container_doc
        .descendants()
        .find(|node| node.has_tag_name("rootfile"))
        .and_then(|node| node.attribute("full-path"))
        .ok_or(MusicXmlError::Unsupported(
            "compressed MusicXML has no rootfile",
        ))?;
    if rootfile.starts_with('/') || rootfile.split('/').any(|segment| segment == "..") {
        return Err(MusicXmlError::Unsupported(
            "unsafe rootfile path in compressed MusicXML",
        ));
    }
    let entry = archive
        .by_name(rootfile)
        .map_err(|_| MusicXmlError::Unsupported("MusicXML rootfile is missing from archive"))?;
    if entry.size() > MAX_XML_BYTES {
        return Err(MusicXmlError::Unsupported(
            "compressed MusicXML score is too large",
        ));
    }
    let mut xml = String::new();
    entry
        .take(MAX_XML_BYTES)
        .read_to_string(&mut xml)
        .map_err(|_| MusicXmlError::Unsupported("could not read compressed MusicXML score"))?;
    import(&xml)
}

fn part_id(index: usize) -> String {
    format!("P{}", index + 1)
}

fn escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn clef(clef: Clef) -> (&'static str, u8) {
    match clef {
        Clef::Treble => ("G", 2),
        Clef::Bass => ("F", 4),
        Clef::Alto => ("C", 3),
        Clef::Tenor => ("C", 4),
    }
}

fn note_type(value: NoteValue) -> &'static str {
    match value {
        NoteValue::Whole => "whole",
        NoteValue::Half => "half",
        NoteValue::Quarter => "quarter",
        NoteValue::Eighth => "eighth",
        NoteValue::Sixteenth => "16th",
        NoteValue::ThirtySecond => "32nd",
        NoteValue::SixtyFourth => "64th",
    }
}

fn step(value: notula_model::Step) -> char {
    match value {
        notula_model::Step::C => 'C',
        notula_model::Step::D => 'D',
        notula_model::Step::E => 'E',
        notula_model::Step::F => 'F',
        notula_model::Step::G => 'G',
        notula_model::Step::A => 'A',
        notula_model::Step::B => 'B',
    }
}

fn write_event(xml: &mut String, event: &Event, voice: usize, staff: usize, fifths: i8) {
    let pitches = match &event.kind {
        EventKind::Rest => vec![None],
        EventKind::Chord { pitches, .. } => pitches.iter().copied().map(Some).collect(),
    };
    let (value, dots) = (event.duration.value, event.duration.dots);
    for (pitch_index, pitch) in pitches.into_iter().enumerate() {
        xml.push_str("      <note>");
        if pitch_index > 0 {
            xml.push_str("<chord/>");
        }
        if let Some(pitch) = pitch {
            let _ = write!(
                xml,
                "<pitch><step>{}</step><alter>{}</alter><octave>{}</octave></pitch>",
                step(pitch.step),
                pitch.alter,
                pitch.octave
            );
        } else {
            xml.push_str("<rest/>");
        }
        let _ = write!(xml, "<duration>{}</duration>", event.duration.ticks());
        let _ = write!(
            xml,
            "<voice>{voice}</voice><type>{}</type>",
            note_type(value)
        );
        for _ in 0..dots {
            xml.push_str("<dot/>");
        }
        if matches!(event.kind, EventKind::Chord { tied: true, .. }) && pitch.is_some() {
            xml.push_str("<tie type=\"start\"/>");
        }
        let _ = write!(xml, "<staff>{staff}</staff>");
        write_markings(xml, event, fifths, pitch_index == 0);
        if let Some(pitch) = pitch {
            if pitch.alter != (notula_model::KeySignature { fifths }).alter(pitch.step) {
                let accidental = match pitch.alter {
                    -2 => "flat-flat",
                    -1 => "flat",
                    0 => "natural",
                    1 => "sharp",
                    2 => "double-sharp",
                    _ => "natural",
                };
                let _ = write!(xml, "<accidental>{accidental}</accidental>");
            }
        }
        xml.push_str("</note>\n");
    }
}

fn write_markings(xml: &mut String, event: &Event, _fifths: i8, include_articulations: bool) {
    let articulations = &event.markings.articulations;
    let fermata = articulations.contains(&Articulation::Fermata);
    let ordinary: Vec<&str> = articulations
        .iter()
        .filter_map(|item| match item {
            Articulation::Staccato => Some("staccato"),
            Articulation::Staccatissimo => Some("staccatissimo"),
            Articulation::Tenuto => Some("tenuto"),
            Articulation::Accent => Some("accent"),
            Articulation::Marcato => Some("strong-accent"),
            Articulation::Fermata => None,
        })
        .collect();
    let tied = matches!(event.kind, EventKind::Chord { tied: true, .. });
    if (include_articulations && (!ordinary.is_empty() || fermata)) || tied {
        xml.push_str("<notations>");
        if tied {
            xml.push_str("<tied type=\"start\"/>");
        }
        if include_articulations && fermata {
            xml.push_str("<fermata type=\"upright\"/>");
        }
        if include_articulations && !ordinary.is_empty() {
            xml.push_str("<articulations>");
            for name in ordinary {
                let _ = write!(xml, "<{name}/>");
            }
            xml.push_str("</articulations>");
        }
        xml.push_str("</notations>");
    }
}

fn write_directions(xml: &mut String, event: &Event, staff: usize) {
    if let Some(dynamic) = event.markings.dynamic {
        let name = match dynamic {
            Dynamic::Ppp => "ppp",
            Dynamic::Pp => "pp",
            Dynamic::P => "p",
            Dynamic::Mp => "mp",
            Dynamic::Mf => "mf",
            Dynamic::F => "f",
            Dynamic::Ff => "ff",
            Dynamic::Fff => "fff",
            Dynamic::Fp => "fp",
            Dynamic::Sf => "sf",
            Dynamic::Sfz => "sfz",
            Dynamic::Rfz => "rfz",
        };
        let _ = write!(
            xml,
            "      <direction placement=\"below\"><direction-type><dynamics><{name}/></dynamics></direction-type><staff>{staff}</staff></direction>\n"
        );
    }
    if let Some(hairpin) = event.markings.hairpin {
        let wedge = match hairpin {
            Hairpin::Crescendo => "crescendo",
            Hairpin::Diminuendo => "diminuendo",
            Hairpin::End => "stop",
        };
        let _ = write!(
            xml,
            "      <direction><direction-type><wedge type=\"{wedge}\"/></direction-type><staff>{staff}</staff></direction>\n"
        );
    }
}

#[derive(Clone)]
struct ParsedEvent {
    staff: usize,
    voice: usize,
    onset: u32,
    duration: Duration,
    kind: EventKind,
    markings: Markings,
}

#[derive(Default)]
struct ParsedMeasure {
    time: Option<(TimeSignature, TimeSignatureDisplay)>,
    key: Option<KeySignature>,
    implicit: bool,
    staves: usize,
    clefs: BTreeMap<usize, Clef>,
    events: Vec<ParsedEvent>,
    end_tick: u32,
}

struct ParsedPart {
    name: String,
    short_name: String,
    clefs: Vec<Clef>,
    measures: Vec<ParsedMeasure>,
}

#[derive(Clone, Copy)]
struct ParsedDirection {
    staff: usize,
    onset: u32,
    dynamic: Option<Dynamic>,
    hairpin: Option<Hairpin>,
}

fn element<'a, 'input>(
    node: roxmltree::Node<'a, 'input>,
    name: &str,
) -> Option<roxmltree::Node<'a, 'input>> {
    node.children()
        .find(|child| child.is_element() && child.tag_name().name() == name)
}

fn elements<'a, 'input>(
    node: roxmltree::Node<'a, 'input>,
    name: &str,
) -> impl Iterator<Item = roxmltree::Node<'a, 'input>> {
    node.children()
        .filter(move |child| child.is_element() && child.tag_name().name() == name)
}

fn child_text<'a, 'input>(node: roxmltree::Node<'a, 'input>, name: &str) -> Option<&'a str> {
    element(node, name)?.text()
}

fn parse_number(node: roxmltree::Node<'_, '_>, name: &str) -> Option<u32> {
    child_text(node, name)?.trim().parse().ok()
}

fn remember(unsupported: &mut BTreeSet<String>, feature: impl Into<String>) {
    unsupported.insert(feature.into());
}

fn without_doctype(xml: &str) -> Cow<'_, str> {
    let bytes = xml.as_bytes();
    let mut cursor = 0;
    let mut doctype = None;
    while cursor < bytes.len() {
        if bytes[cursor..].starts_with(b"<!--") {
            cursor = find_bytes(bytes, cursor + 4, b"-->").unwrap_or(bytes.len());
            continue;
        }
        if bytes[cursor..].starts_with(b"<![CDATA[") {
            cursor = find_bytes(bytes, cursor + 9, b"]]>").unwrap_or(bytes.len());
            continue;
        }
        if bytes[cursor..].starts_with(b"<?") {
            cursor = find_bytes(bytes, cursor + 2, b"?>").unwrap_or(bytes.len());
            continue;
        }
        if !bytes[cursor..].starts_with(b"<!DOCTYPE") {
            cursor += 1;
            continue;
        }

        let start = cursor;
        cursor += b"<!DOCTYPE".len();
        let mut subset_depth = 0_u32;
        let mut quote = None;
        let mut comment = false;
        while cursor < bytes.len() {
            if comment {
                if bytes[cursor..].starts_with(b"-->") {
                    comment = false;
                    cursor += 3;
                } else {
                    cursor += 1;
                }
                continue;
            }
            if quote.is_none() && bytes[cursor..].starts_with(b"<!--") {
                comment = true;
                cursor += 4;
                continue;
            }
            let byte = bytes[cursor];
            if let Some(delimiter) = quote {
                if byte == delimiter {
                    quote = None;
                }
            } else {
                match byte {
                    b'\'' | b'"' => quote = Some(byte),
                    b'[' => subset_depth += 1,
                    b']' => subset_depth = subset_depth.saturating_sub(1),
                    b'>' if subset_depth == 0 => {
                        cursor += 1;
                        doctype = Some((start, cursor));
                        break;
                    }
                    _ => {}
                }
            }
            cursor += 1;
        }
        if doctype.is_some() || cursor >= bytes.len() {
            break;
        }
    }

    if let Some((start, end)) = doctype {
        let mut sanitized = String::with_capacity(xml.len() - (end - start));
        sanitized.push_str(&xml[..start]);
        sanitized.push_str(&xml[end..]);
        Cow::Owned(sanitized)
    } else {
        Cow::Borrowed(xml)
    }
}

fn find_bytes(haystack: &[u8], start: usize, needle: &[u8]) -> Option<usize> {
    haystack[start..]
        .windows(needle.len())
        .position(|window| window == needle)
        .map(|offset| start + offset + needle.len())
}

fn parse_clef(node: roxmltree::Node<'_, '_>) -> Option<(usize, Clef)> {
    let sign = child_text(node, "sign")?;
    let line: u8 = child_text(node, "line")?.parse().ok()?;
    let clef = match (sign, line) {
        ("G", 2) => Clef::Treble,
        ("F", 4) => Clef::Bass,
        ("C", 3) => Clef::Alto,
        ("C", 4) => Clef::Tenor,
        _ => return None,
    };
    let number: usize = node
        .attribute("number")
        .and_then(|value| value.parse().ok())
        .unwrap_or(1);
    Some((number.saturating_sub(1), clef))
}

fn parse_step(value: &str) -> Option<Step> {
    match value {
        "C" => Some(Step::C),
        "D" => Some(Step::D),
        "E" => Some(Step::E),
        "F" => Some(Step::F),
        "G" => Some(Step::G),
        "A" => Some(Step::A),
        "B" => Some(Step::B),
        _ => None,
    }
}

fn parse_pitch(node: roxmltree::Node<'_, '_>, unsupported: &mut BTreeSet<String>) -> Option<Pitch> {
    let Some(pitch) = element(node, "pitch") else {
        if element(node, "unpitched").is_some() {
            remember(unsupported, "unpitched percussion");
        }
        return None;
    };
    let step = parse_step(child_text(pitch, "step")?)?;
    let alter_value = child_text(pitch, "alter")
        .and_then(|value| value.parse::<f64>().ok())
        .unwrap_or(0.0);
    if alter_value.fract() != 0.0 {
        remember(unsupported, "microtonal pitches (rounded to semitones)");
    }
    let alter = alter_value.round().clamp(-2.0, 2.0) as i8;
    if alter_value.round() != f64::from(alter) {
        remember(
            unsupported,
            "alterations beyond double accidentals (clamped)",
        );
    }
    let pitch = Pitch::new(step, alter, child_text(pitch, "octave")?.parse().ok()?);
    if pitch.validate().is_err() {
        remember(unsupported, "pitches outside Notula's supported MIDI range");
        return None;
    }
    Some(pitch)
}

fn note_value_from_name(name: &str) -> Option<NoteValue> {
    match name {
        "whole" => Some(NoteValue::Whole),
        "half" => Some(NoteValue::Half),
        "quarter" => Some(NoteValue::Quarter),
        "eighth" => Some(NoteValue::Eighth),
        "16th" => Some(NoteValue::Sixteenth),
        "32nd" => Some(NoteValue::ThirtySecond),
        "64th" => Some(NoteValue::SixtyFourth),
        _ => None,
    }
}

fn duration_from_ticks(ticks: u32, unsupported: &mut BTreeSet<String>) -> Duration {
    let values = [
        NoteValue::Whole,
        NoteValue::Half,
        NoteValue::Quarter,
        NoteValue::Eighth,
        NoteValue::Sixteenth,
        NoteValue::ThirtySecond,
        NoteValue::SixtyFourth,
    ];
    let candidates: Vec<_> = values
        .into_iter()
        .flat_map(|value| (0..=2).map(move |dots| Duration::new(value, dots)))
        .collect();
    if let Some(duration) = candidates
        .iter()
        .copied()
        .find(|duration| duration.ticks() == ticks)
    {
        return duration;
    }
    remember(
        unsupported,
        format!("non-standard duration of {ticks} ticks (approximated)"),
    );
    candidates
        .into_iter()
        .min_by_key(|duration| duration.ticks().abs_diff(ticks))
        .unwrap_or(Duration::new(NoteValue::Quarter, 0))
}

fn parse_duration(
    node: roxmltree::Node<'_, '_>,
    divisions: u32,
    unsupported: &mut BTreeSet<String>,
) -> Option<(Duration, u32)> {
    if element(node, "grace").is_some() {
        remember(unsupported, "grace notes");
        return None;
    }
    if let Some(raw) = parse_number(node, "duration") {
        let divisions = divisions.max(1);
        let ticks = ((u64::from(raw) * u64::from(TICKS_PER_QUARTER) + u64::from(divisions / 2))
            / u64::from(divisions)) as u32;
        return Some((duration_from_ticks(ticks, unsupported), ticks));
    }
    let value = child_text(node, "type").and_then(note_value_from_name);
    if let Some(value) = value {
        let dots = elements(node, "dot").count().min(2) as u8;
        return Some((
            Duration::new(value, dots),
            Duration::new(value, dots).ticks(),
        ));
    }
    remember(unsupported, "notes without a supported duration");
    None
}

fn parse_dynamic(node: roxmltree::Node<'_, '_>) -> Option<Dynamic> {
    match node.tag_name().name() {
        "ppp" => Some(Dynamic::Ppp),
        "pp" => Some(Dynamic::Pp),
        "p" => Some(Dynamic::P),
        "mp" => Some(Dynamic::Mp),
        "mf" => Some(Dynamic::Mf),
        "f" => Some(Dynamic::F),
        "ff" => Some(Dynamic::Ff),
        "fff" => Some(Dynamic::Fff),
        "fp" => Some(Dynamic::Fp),
        "sf" => Some(Dynamic::Sf),
        "sfz" => Some(Dynamic::Sfz),
        "rfz" => Some(Dynamic::Rfz),
        _ => None,
    }
}

fn parse_markings(node: roxmltree::Node<'_, '_>, unsupported: &mut BTreeSet<String>) -> Markings {
    let mut markings = Markings::default();
    for child in node.children().filter(|child| child.is_element()) {
        match child.tag_name().name() {
            "beam" => {
                remember(unsupported, "explicit beam grouping");
            }
            "time-modification" => {
                remember(unsupported, "tuplets");
            }
            "lyric" => {
                remember(unsupported, "lyrics");
            }
            "instrument" => {
                remember(unsupported, "instrument assignments");
            }
            "stem" => {
                remember(unsupported, "explicit stem directions");
            }
            "notehead" => {
                remember(unsupported, "custom noteheads");
            }
            "play" => {
                remember(unsupported, "playback directives");
            }
            _ => {}
        }
    }
    for notation in elements(node, "notations") {
        for articulation_set in elements(notation, "articulations") {
            for item in articulation_set
                .children()
                .filter(|child| child.is_element())
            {
                let articulation = match item.tag_name().name() {
                    "staccato" => Some(Articulation::Staccato),
                    "staccatissimo" => Some(Articulation::Staccatissimo),
                    "tenuto" => Some(Articulation::Tenuto),
                    "accent" => Some(Articulation::Accent),
                    "strong-accent" => Some(Articulation::Marcato),
                    name => {
                        remember(unsupported, format!("articulation: {name}"));
                        None
                    }
                };
                if let Some(articulation) = articulation {
                    markings.articulations.push(articulation);
                }
            }
        }
        if elements(notation, "fermata").next().is_some() {
            markings.articulations.push(Articulation::Fermata);
        }
        for tied in elements(notation, "tied") {
            if tied.attribute("type") == Some("start") {
                // Ties are stored on the note that starts the tie.
            }
        }
        for child in notation.children().filter(|child| child.is_element()) {
            if !matches!(
                child.tag_name().name(),
                "articulations" | "fermata" | "tied" | "slur"
            ) {
                remember(
                    unsupported,
                    format!("notation: {}", child.tag_name().name()),
                );
            }
            if child.has_tag_name("slur") {
                remember(unsupported, "slurs");
            }
        }
    }
    markings
}

fn parse_measure(
    measure: roxmltree::Node<'_, '_>,
    divisions: &mut u32,
    voice_names: &mut BTreeMap<String, usize>,
    unsupported: &mut BTreeSet<String>,
) -> ParsedMeasure {
    let mut result = ParsedMeasure {
        implicit: measure.attribute("implicit") == Some("yes"),
        ..ParsedMeasure::default()
    };
    let mut cursor: i64 = 0;
    let mut previous_chord: HashMap<(usize, usize, u32), usize> = HashMap::new();
    let mut directions = Vec::new();
    for child in measure.children().filter(|child| child.is_element()) {
        match child.tag_name().name() {
            "attributes" => {
                if let Some(value) = parse_number(child, "divisions") {
                    *divisions = value.max(1);
                }
                if let Some(key) = element(child, "key") {
                    if let Some(fifths) =
                        child_text(key, "fifths").and_then(|v| v.parse::<i8>().ok())
                    {
                        result.key = Some(KeySignature { fifths });
                    }
                }
                if let Some(time_node) = element(child, "time") {
                    let beats = child_text(time_node, "beats").and_then(|v| v.parse::<u8>().ok());
                    let beat_type =
                        child_text(time_node, "beat-type").and_then(|v| v.parse::<u8>().ok());
                    if let (Some(numerator), Some(denominator)) = (beats, beat_type) {
                        let time = TimeSignature {
                            numerator,
                            denominator,
                        };
                        if time.validate().is_ok() {
                            let display = match time_node.attribute("symbol") {
                                Some("common") if time == TimeSignature::COMMON => {
                                    TimeSignatureDisplay::Common
                                }
                                Some("cut") if time.numerator == 2 && time.denominator == 2 => {
                                    TimeSignatureDisplay::AllaBreve
                                }
                                _ => TimeSignatureDisplay::Numeric,
                            };
                            result.time = Some((time, display));
                        } else {
                            remember(
                                unsupported,
                                format!("time signature {numerator}/{denominator}"),
                            );
                        }
                    }
                }
                if let Some(staves) = parse_number(child, "staves") {
                    result.staves = result.staves.max(staves as usize);
                }
                for clef_node in elements(child, "clef") {
                    let number = clef_node
                        .attribute("number")
                        .and_then(|v| v.parse::<usize>().ok())
                        .unwrap_or(1)
                        .saturating_sub(1);
                    result.staves = result.staves.max(number + 1);
                    if let Some((staff, clef)) = parse_clef(clef_node) {
                        result.clefs.insert(staff, clef);
                    } else {
                        remember(unsupported, "clef type");
                    }
                }
            }
            "backup" | "forward" => {
                let raw = parse_number(child, "duration").unwrap_or(0);
                let ticks = (u64::from(raw) * u64::from(TICKS_PER_QUARTER)
                    / u64::from((*divisions).max(1))) as i64;
                if child.has_tag_name("backup") {
                    cursor = (cursor - ticks).max(0);
                } else {
                    cursor += ticks;
                }
            }
            "note" => {
                let staff = parse_number(child, "staff").unwrap_or(1).saturating_sub(1) as usize;
                result.staves = result.staves.max(staff + 1);
                let voice_name = child_text(child, "voice").unwrap_or("1").trim().to_owned();
                let voice = if let Ok(number) = voice_name.parse::<usize>() {
                    number.saturating_sub(1)
                } else {
                    let next = voice_names.len();
                    *voice_names.entry(voice_name).or_insert(next)
                };
                let Some((duration, raw_ticks)) = parse_duration(child, *divisions, unsupported)
                else {
                    continue;
                };
                let is_chord_tone = element(child, "chord").is_some();
                let onset = if is_chord_tone {
                    (cursor - i64::from(raw_ticks)).max(0) as u32
                } else {
                    cursor.max(0) as u32
                };
                result.end_tick = result.end_tick.max(onset.saturating_add(raw_ticks));
                let tied = child.children().any(|node| {
                    node.is_element()
                        && node.has_tag_name("tie")
                        && node.attribute("type") == Some("start")
                }) || elements(child, "notations").any(|notation| {
                    elements(notation, "tied").any(|tie| tie.attribute("type") == Some("start"))
                });
                let markings = parse_markings(child, unsupported);
                let kind = if element(child, "rest").is_some() {
                    EventKind::Rest
                } else if let Some(pitch) = parse_pitch(child, unsupported) {
                    EventKind::Chord {
                        pitches: vec![pitch],
                        tied,
                    }
                } else {
                    remember(unsupported, "notes without a standard pitch");
                    continue;
                };
                let key = (staff, voice, onset);
                if is_chord_tone {
                    if let Some(&index) = previous_chord.get(&key) {
                        if let (
                            EventKind::Chord {
                                pitches: existing,
                                tied: old_tie,
                            },
                            EventKind::Chord { pitches, tied },
                        ) = (&mut result.events[index].kind, kind)
                        {
                            existing.extend(pitches);
                            *old_tie |= tied;
                            result.events[index]
                                .markings
                                .articulations
                                .extend(markings.articulations);
                        } else {
                            remember(unsupported, "chord markers on non-chord events");
                        }
                    } else {
                        remember(unsupported, "chord tones without a preceding note");
                    }
                } else {
                    let index = result.events.len();
                    result.events.push(ParsedEvent {
                        staff,
                        voice,
                        onset,
                        duration,
                        kind,
                        markings,
                    });
                    previous_chord.insert(key, index);
                    cursor += i64::from(raw_ticks);
                }
            }
            "direction" => {
                let staff = parse_number(child, "staff").unwrap_or(1).saturating_sub(1) as usize;
                let offset = parse_number(child, "offset").unwrap_or(0);
                let offset_ticks = (u64::from(offset) * u64::from(TICKS_PER_QUARTER)
                    / u64::from((*divisions).max(1))) as u32;
                let mut direction = ParsedDirection {
                    staff,
                    onset: (cursor.max(0) as u32).saturating_add(offset_ticks),
                    dynamic: None,
                    hairpin: None,
                };
                for direction_type in elements(child, "direction-type") {
                    if let Some(dynamics) = element(direction_type, "dynamics") {
                        direction.dynamic = dynamics
                            .children()
                            .find(|n| n.is_element())
                            .and_then(parse_dynamic);
                        if direction.dynamic.is_none() {
                            remember(unsupported, "dynamic marking");
                        }
                    }
                    if let Some(wedge) = element(direction_type, "wedge") {
                        direction.hairpin = match wedge.attribute("type") {
                            Some("crescendo") => Some(Hairpin::Crescendo),
                            Some("diminuendo") => Some(Hairpin::Diminuendo),
                            Some("stop") => Some(Hairpin::End),
                            _ => {
                                remember(unsupported, "wedge type");
                                None
                            }
                        };
                    }
                    for item in direction_type.children().filter(|n| n.is_element()) {
                        if !matches!(item.tag_name().name(), "dynamics" | "wedge") {
                            remember(
                                unsupported,
                                format!("direction: {}", item.tag_name().name()),
                            );
                        }
                    }
                }
                if direction.dynamic.is_some() || direction.hairpin.is_some() {
                    directions.push(direction);
                }
            }
            "barline" => remember(unsupported, "barline styles and repeats"),
            "print" | "sound" | "harmony" | "figured-bass" | "grouping" => remember(
                unsupported,
                format!("measure element: {}", child.tag_name().name()),
            ),
            _ => remember(
                unsupported,
                format!("measure element: {}", child.tag_name().name()),
            ),
        }
    }
    for direction in directions {
        if let Some(event) = result
            .events
            .iter_mut()
            .filter(|event| event.staff == direction.staff && event.onset >= direction.onset)
            .min_by_key(|event| event.onset)
        {
            if direction.dynamic.is_some() {
                event.markings.dynamic = direction.dynamic;
            }
            if direction.hairpin.is_some() {
                event.markings.hairpin = direction.hairpin;
            }
        } else {
            remember(unsupported, "direction with no following note");
        }
    }
    result
}

fn parse_partwise_part(
    part: roxmltree::Node<'_, '_>,
    name: String,
    short_name: String,
    unsupported: &mut BTreeSet<String>,
) -> ParsedPart {
    let mut divisions = TICKS_PER_QUARTER;
    let mut voice_names = BTreeMap::new();
    let measures: Vec<_> = elements(part, "measure")
        .map(|measure| parse_measure(measure, &mut divisions, &mut voice_names, unsupported))
        .collect();
    let staff_count = measures
        .iter()
        .map(|measure| measure.staves)
        .max()
        .unwrap_or(1)
        .max(1);
    let clefs = (0..staff_count)
        .map(|staff| {
            measures
                .iter()
                .find_map(|measure| measure.clefs.get(&staff))
                .copied()
                .unwrap_or(Clef::Treble)
        })
        .collect();
    ParsedPart {
        name,
        short_name,
        clefs,
        measures,
    }
}

fn insert_rest_gap(
    doc: &mut Document,
    at: VoiceLocation,
    mut ticks: u32,
) -> Result<(), MusicXmlError> {
    let values = [
        NoteValue::Whole,
        NoteValue::Half,
        NoteValue::Quarter,
        NoteValue::Eighth,
        NoteValue::Sixteenth,
        NoteValue::ThirtySecond,
        NoteValue::SixtyFourth,
    ];
    let mut index = doc.score().voice(at)?.events.len();
    while ticks > 0 {
        let duration = values
            .into_iter()
            .flat_map(|value| (0..=2).map(move |dots| Duration::new(value, dots)))
            .filter(|duration| duration.ticks() <= ticks)
            .max_by_key(|duration| duration.ticks())
            .ok_or(MusicXmlError::Unsupported(
                "rest gap smaller than the supported rhythmic grid",
            ))?;
        let event = Event::new(doc.new_event_id(), duration, EventKind::Rest);
        doc.execute(Command::InsertEvent { at, index, event })?;
        index += 1;
        ticks -= duration.ticks();
    }
    Ok(())
}

pub fn import(xml: &str) -> Result<MusicXmlImport, MusicXmlError> {
    let sanitized = without_doctype(xml);
    let document = roxmltree::Document::parse(&sanitized)?;
    let root = document.root_element();
    if !root.has_tag_name("score-partwise") {
        return Err(MusicXmlError::Unsupported(
            "only score-partwise MusicXML is supported",
        ));
    }
    let mut unsupported = BTreeSet::new();
    let names: HashMap<String, (String, String)> = element(root, "part-list")
        .into_iter()
        .flat_map(|list| elements(list, "score-part"))
        .map(|part| {
            let id = part.attribute("id").unwrap_or("").to_owned();
            let name = child_text(part, "part-name").unwrap_or("Part").to_owned();
            let short = child_text(part, "part-abbreviation")
                .unwrap_or("")
                .to_owned();
            (id, (name, short))
        })
        .collect();
    let parsed_parts: Vec<_> = elements(root, "part")
        .map(|part| {
            let id = part.attribute("id").unwrap_or("");
            let (name, short) = names
                .get(id)
                .cloned()
                .unwrap_or_else(|| (format!("Part {}", id), String::new()));
            parse_partwise_part(part, name, short, &mut unsupported)
        })
        .collect();
    if parsed_parts.is_empty() {
        return Err(MusicXmlError::Unsupported("score has no parts"));
    }
    let measure_count = parsed_parts
        .iter()
        .map(|part| part.measures.len())
        .max()
        .unwrap_or(1)
        .max(1);
    let mut measure_info = Vec::with_capacity(measure_count);
    let mut time = (TimeSignature::COMMON, TimeSignatureDisplay::Numeric);
    let mut key = KeySignature::default();
    for index in 0..measure_count {
        let source = parsed_parts
            .iter()
            .find_map(|part| part.measures.get(index));
        if let Some(source) = source {
            if let Some(next) = source.time {
                time = next;
            }
            if let Some(next) = source.key {
                key = next;
            }
        }
        let capacity = time.0.ticks_per_measure();
        let pickup_end = parsed_parts
            .iter()
            .filter_map(|part| part.measures.get(index))
            .filter(|measure| measure.implicit)
            .map(|measure| measure.end_tick)
            .max()
            .unwrap_or(capacity);
        let length = if pickup_end > 0
            && pickup_end < capacity
            && pickup_end.is_multiple_of(NoteValue::SixtyFourth.ticks())
        {
            Some(pickup_end)
        } else {
            None
        };
        measure_info.push(MeasureInfo {
            time_signature: time.0,
            time_signature_display: time.1,
            key_signature: key,
            length,
        });
    }

    let mut doc = Document::new();
    let title = element(root, "work")
        .and_then(|work| child_text(work, "work-title"))
        .unwrap_or("")
        .to_owned();
    let composer = element(root, "identification")
        .and_then(|ident| {
            elements(ident, "creator").find(|creator| creator.attribute("type") == Some("composer"))
        })
        .and_then(|creator| creator.text())
        .unwrap_or("")
        .to_owned();
    doc.execute(Command::SetMetadata { title, composer })?;
    for (index, part) in parsed_parts.iter().enumerate() {
        let mut model_part = doc.new_part(&part.name, &part.clefs);
        model_part.short_name = part.short_name.clone();
        doc.execute(Command::AddPart {
            index,
            part: model_part,
        })?;
    }
    for index in 1..measure_count {
        let data = doc.score().blank_measure(index);
        doc.execute(Command::InsertMeasure { index, data })?;
    }
    for (index, info) in measure_info.iter().copied().enumerate() {
        doc.execute(Command::SetTimeSignature {
            measure: index,
            time_signature: info.time_signature,
        })?;
        doc.execute(Command::SetTimeSignatureDisplay {
            measure: index,
            display: info.time_signature_display,
        })?;
        doc.execute(Command::SetKeySignature {
            measure: index,
            key_signature: info.key_signature,
        })?;
        doc.execute(Command::SetMeasureLength {
            measure: index,
            length: info.length,
        })?;
    }

    for (part_index, part) in parsed_parts.iter().enumerate() {
        let staff_ids: Vec<_> = doc.score().parts[part_index]
            .staves
            .iter()
            .map(|staff| staff.id)
            .collect();
        for (staff_index, staff_id) in staff_ids.iter().copied().enumerate() {
            let voice_count = part
                .measures
                .iter()
                .flat_map(|measure| &measure.events)
                .filter(|event| event.staff == staff_index)
                .map(|event| event.voice + 1)
                .max()
                .unwrap_or(1);
            for voice in 1..voice_count {
                doc.execute(Command::InsertVoice {
                    staff: staff_id,
                    measure: 0,
                    index: voice,
                    voices: vec![Voice::default(); measure_count],
                })?;
            }
            for measure_index in 0..measure_count {
                let Some(source) = part.measures.get(measure_index) else {
                    continue;
                };
                for voice_index in 0..voice_count {
                    let at = VoiceLocation {
                        staff: staff_id,
                        measure: measure_index,
                        voice: voice_index,
                    };
                    let mut events: Vec<_> = source
                        .events
                        .iter()
                        .filter(|event| event.staff == staff_index && event.voice == voice_index)
                        .cloned()
                        .collect();
                    events.sort_by_key(|event| event.onset);
                    let capacity = doc.score().measure_capacity(measure_index)?;
                    let mut onset = 0;
                    for parsed in events {
                        let target = parsed.onset.min(capacity);
                        if target < onset {
                            remember(&mut unsupported, "overlapping events in one voice");
                            continue;
                        }
                        let gap = target - onset;
                        let snapped_gap =
                            gap / NoteValue::SixtyFourth.ticks() * NoteValue::SixtyFourth.ticks();
                        if snapped_gap != gap {
                            remember(
                                &mut unsupported,
                                "event onset outside the supported rhythmic grid",
                            );
                        }
                        insert_rest_gap(&mut doc, at, snapped_gap)?;
                        onset += snapped_gap;
                        if onset.saturating_add(parsed.duration.ticks()) > capacity {
                            remember(&mut unsupported, "notes extending beyond measure capacity");
                            continue;
                        }
                        let event = Event {
                            id: doc.new_event_id(),
                            duration: parsed.duration,
                            kind: parsed.kind,
                            markings: parsed.markings,
                        };
                        let index = doc.score().voice(at)?.events.len();
                        doc.execute(Command::InsertEvent { at, index, event })?;
                        onset += parsed.duration.ticks();
                    }
                }
            }
        }
    }
    doc.score().validate()?;
    Ok(MusicXmlImport {
        score: doc.into_score(),
        unsupported: unsupported.into_iter().collect(),
    })
}

#[cfg(test)]
mod tests {
    use notula_model::{
        Command, Document, Duration, Event, EventKind, NoteValue, Pitch, Step, VoiceLocation,
    };

    use super::*;

    #[test]
    fn export_contains_escaped_metadata_chords_and_markings() {
        let mut doc = Document::new();
        let mut part = doc.new_part("Piano & organ", &[Clef::Treble, Clef::Bass]);
        part.short_name = "Pno.".to_owned();
        doc.execute(Command::AddPart { index: 0, part }).unwrap();
        doc.execute(Command::SetMetadata {
            title: "Mass <I>".to_owned(),
            composer: "A & B".to_owned(),
        })
        .unwrap();
        let event_id = doc.new_event_id();
        let mut event = Event::new(
            event_id,
            Duration::new(NoteValue::Quarter, 1),
            EventKind::Chord {
                pitches: vec![Pitch::new(Step::C, 0, 4), Pitch::new(Step::E, 0, 4)],
                tied: true,
            },
        );
        event.markings.articulations = vec![Articulation::Accent, Articulation::Fermata];
        let at = VoiceLocation {
            staff: doc.score().parts[0].staves[0].id,
            measure: 0,
            voice: 0,
        };
        doc.execute(Command::InsertEvent {
            at,
            index: 0,
            event,
        })
        .unwrap();

        let result = export(doc.score());
        let xml = roxmltree::Document::parse(&result.xml).unwrap();
        assert_eq!(xml.root_element().tag_name().name(), "score-partwise");
        assert_eq!(
            xml.descendants()
                .find(|node| node.has_tag_name("work-title"))
                .unwrap()
                .text(),
            Some("Mass <I>")
        );
        assert_eq!(
            xml.descendants()
                .filter(|node| node.has_tag_name("note"))
                .count(),
            2
        );
        assert_eq!(
            xml.descendants()
                .filter(|node| node.has_tag_name("chord"))
                .count(),
            1
        );
        assert_eq!(
            xml.descendants()
                .filter(|node| node.has_tag_name("tie"))
                .count(),
            2
        );
        assert_eq!(
            xml.descendants()
                .filter(|node| node.has_tag_name("tied"))
                .count(),
            2
        );
        assert_eq!(
            xml.descendants()
                .filter(|node| node.has_tag_name("fermata"))
                .count(),
            1
        );
        assert!(result.unsupported.is_empty());
    }

    #[test]
    fn exported_scores_import_with_supported_notation_intact() {
        let mut doc = Document::new();
        let part = doc.new_part("Piano", &[Clef::Treble, Clef::Bass]);
        doc.execute(Command::AddPart { index: 0, part }).unwrap();
        doc.execute(Command::SetMetadata {
            title: "Suite".to_owned(),
            composer: "Composer".to_owned(),
        })
        .unwrap();
        let event = Event {
            id: doc.new_event_id(),
            duration: Duration::new(NoteValue::Eighth, 1),
            kind: EventKind::Chord {
                pitches: vec![Pitch::new(Step::G, 1, 5), Pitch::new(Step::B, 0, 5)],
                tied: true,
            },
            markings: Markings {
                articulations: vec![Articulation::Marcato],
                dynamic: Some(Dynamic::Mf),
                hairpin: Some(Hairpin::Crescendo),
            },
        };
        let at = VoiceLocation {
            staff: doc.score().parts[0].staves[0].id,
            measure: 0,
            voice: 0,
        };
        doc.execute(Command::InsertEvent {
            at,
            index: 0,
            event,
        })
        .unwrap();

        let exported = export(doc.score());
        let imported = import(&exported.xml).unwrap();
        let score = imported.score;
        assert_eq!(score.title, "Suite");
        assert_eq!(score.composer, "Composer");
        assert_eq!(score.parts[0].name, "Piano");
        assert_eq!(score.parts[0].staves.len(), 2);
        let imported_event = &score.parts[0].staves[0].measures[0].voices[0].events[0];
        assert_eq!(imported_event.duration, Duration::new(NoteValue::Eighth, 1));
        assert_eq!(
            imported_event.kind,
            EventKind::Chord {
                pitches: vec![Pitch::new(Step::G, 1, 5), Pitch::new(Step::B, 0, 5)],
                tied: true,
            }
        );
        assert_eq!(imported_event.markings.dynamic, Some(Dynamic::Mf));
        assert_eq!(imported_event.markings.hairpin, Some(Hairpin::Crescendo));
        assert_eq!(
            imported_event.markings.articulations,
            vec![Articulation::Marcato]
        );
        assert!(imported.unsupported.is_empty());
    }

    #[test]
    fn import_reports_unsupported_notations() {
        let xml = r#"<score-partwise version="4.0"><part-list><score-part id="P1"><part-name>Part</part-name></score-part></part-list><part id="P1"><measure number="1"><attributes><divisions>1</divisions><time><beats>4</beats><beat-type>4</beat-type></time></attributes><note><pitch><step>C</step><octave>4</octave></pitch><duration>1</duration><voice>1</voice><type>quarter</type><notations><slur type="start"/></notations></note></measure></part></score-partwise>"#;
        let imported = import(xml).unwrap();
        assert!(imported.unsupported.iter().any(|item| item == "slurs"));
        assert_eq!(
            imported.score.parts[0].staves[0].measures[0].voices[0]
                .events
                .len(),
            1
        );
    }

    #[test]
    fn import_ignores_musescore_musicxml_doctype() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE score-partwise PUBLIC "-//Recordare//DTD MusicXML 4.0 Partwise//EN" "http://www.musicxml.org/dtds/partwise.dtd">
<score-partwise version="4.0"><part-list><score-part id="P1"><part-name>Part</part-name></score-part></part-list><part id="P1"><measure number="1"><attributes><divisions>1</divisions><time><beats>4</beats><beat-type>4</beat-type></time></attributes><note><rest/><duration>1</duration><voice>1</voice></note></measure></part></score-partwise>"#;
        let imported = import(xml).unwrap();
        assert_eq!(imported.score.parts[0].name, "Part");
        assert!(imported.unsupported.is_empty());
    }

    #[test]
    fn imports_compressed_mxl_container() {
        use std::io::Write as _;

        let mut archive_bytes = std::io::Cursor::new(Vec::new());
        {
            let mut archive = zip::ZipWriter::new(&mut archive_bytes);
            let options = zip::write::SimpleFileOptions::default();
            archive
                .start_file("META-INF/container.xml", options)
                .unwrap();
            archive
                .write_all(b"<container><rootfiles><rootfile full-path=\"scores/main.xml\"/></rootfiles></container>")
                .unwrap();
            archive.start_file("scores/main.xml", options).unwrap();
            archive
                .write_all(br#"<score-partwise><part-list><score-part id="P1"><part-name>Test</part-name></score-part></part-list><part id="P1"><measure number="1"><attributes><divisions>1</divisions><time><beats>4</beats><beat-type>4</beat-type></time><clef><sign>G</sign><line>2</line></clef></attributes><note><rest/><duration>4</duration><voice>1</voice></note></measure></part></score-partwise>"#)
                .unwrap();
            archive.finish().unwrap();
        }
        archive_bytes.set_position(0);
        let imported = import_mxl(archive_bytes).unwrap();
        assert_eq!(imported.score.parts.len(), 1);
        assert_eq!(imported.score.parts[0].name, "Test");
    }
}
