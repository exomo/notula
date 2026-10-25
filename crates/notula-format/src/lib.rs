//! Native `.notula` file format: a zip archive holding a manifest and the score as JSON.

pub mod musicxml;

use std::fs::{self, File};
use std::io::{self, BufReader, Read, Seek, Write};
use std::path::{Path, PathBuf};

use notula_model::{ModelError, Score};
use serde::{Deserialize, Serialize};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

pub const EXTENSION: &str = "notula";
/// Bumped whenever a change would make old readers misinterpret a file.
pub const VERSION: u32 = 1;

const FORMAT_NAME: &str = "notula";
const MANIFEST: &str = "manifest.json";
const SCORE: &str = "score.json";
/// Guards against decompression bombs; real scores are far smaller.
const MAX_ENTRY_BYTES: u64 = 256 * 1024 * 1024;

#[derive(Serialize, Deserialize)]
struct Manifest {
    format: String,
    version: u32,
}

#[derive(Debug, thiserror::Error)]
pub enum FormatError {
    #[error("could not read or write the file: {0}")]
    Io(#[from] io::Error),
    #[error("the file is not a valid archive: {0}")]
    Zip(zip::result::ZipError),
    #[error("the score data is malformed: {0}")]
    Json(#[from] serde_json::Error),
    #[error("this is not a Notula file")]
    NotNotula,
    #[error("the file was written by a newer version (format {found}, supported {supported})")]
    UnsupportedVersion { found: u32, supported: u32 },
    #[error("the file contains too much data")]
    TooLarge,
    #[error("the score is inconsistent: {0}")]
    Invalid(#[from] ModelError),
}

impl From<zip::result::ZipError> for FormatError {
    fn from(error: zip::result::ZipError) -> Self {
        match error {
            zip::result::ZipError::Io(io) => FormatError::Io(io),
            other => FormatError::Zip(other),
        }
    }
}

pub fn write<W: Write + Seek>(score: &Score, writer: W) -> Result<(), FormatError> {
    let mut zip = ZipWriter::new(writer);
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);

    zip.start_file(MANIFEST, options)?;
    let manifest = Manifest {
        format: FORMAT_NAME.to_owned(),
        version: VERSION,
    };
    serde_json::to_writer_pretty(&mut zip, &manifest)?;

    zip.start_file(SCORE, options)?;
    serde_json::to_writer(&mut zip, score)?;

    zip.finish()?;
    Ok(())
}

fn read_entry<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    name: &str,
) -> Result<Vec<u8>, FormatError> {
    let entry = match archive.by_name(name) {
        Ok(entry) => entry,
        Err(zip::result::ZipError::FileNotFound) => return Err(FormatError::NotNotula),
        Err(other) => return Err(other.into()),
    };
    let mut data = Vec::new();
    entry.take(MAX_ENTRY_BYTES + 1).read_to_end(&mut data)?;
    if data.len() as u64 > MAX_ENTRY_BYTES {
        return Err(FormatError::TooLarge);
    }
    Ok(data)
}

pub fn read<R: Read + Seek>(reader: R) -> Result<Score, FormatError> {
    let mut archive = ZipArchive::new(reader)?;

    let manifest: Manifest = serde_json::from_slice(&read_entry(&mut archive, MANIFEST)?)?;
    if manifest.format != FORMAT_NAME {
        return Err(FormatError::NotNotula);
    }
    if manifest.version > VERSION {
        return Err(FormatError::UnsupportedVersion {
            found: manifest.version,
            supported: VERSION,
        });
    }

    let score: Score = serde_json::from_slice(&read_entry(&mut archive, SCORE)?)?;
    score.validate()?;
    Ok(score)
}

/// Writes to a temporary file first so a failed save never damages an existing file.
pub fn save(score: &Score, path: &Path) -> Result<(), FormatError> {
    let mut temp = path.as_os_str().to_owned();
    temp.push(".tmp");
    let temp = PathBuf::from(temp);

    let written = File::create(&temp)
        .map_err(FormatError::from)
        .and_then(|file| {
            write(score, &file)?;
            file.sync_all()?;
            Ok(())
        });
    match written {
        Ok(()) => fs::rename(&temp, path).map_err(|error| {
            let _ = fs::remove_file(&temp);
            error.into()
        }),
        Err(error) => {
            let _ = fs::remove_file(&temp);
            Err(error)
        }
    }
}

pub fn load(path: &Path) -> Result<Score, FormatError> {
    read(BufReader::new(File::open(path)?))
}
