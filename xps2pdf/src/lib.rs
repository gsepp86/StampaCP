//! Libreria di conversione XPS -> PDF.
//!
//! Pipeline prevista:
//! 1. apertura del pacchetto XPS (archivio ZIP) e lettura di FixedDocumentSequence;
//! 2. parsing di ogni FixedPage (percorsi, immagini, glifi con testo Unicode);
//! 3. scrittura del PDF con font incorporati e testo selezionabile.

use std::fmt;
use std::fs;
use std::io;

#[derive(Debug)]
pub enum ConvertError {
    Io(io::Error),
    /// Il contenuto XPS non è valido o non ancora supportato.
    Unsupported(String),
}

impl fmt::Display for ConvertError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConvertError::Io(e) => write!(f, "I/O: {e}"),
            ConvertError::Unsupported(m) => write!(f, "non supportato: {m}"),
        }
    }
}

impl std::error::Error for ConvertError {}

impl From<io::Error> for ConvertError {
    fn from(e: io::Error) -> Self {
        ConvertError::Io(e)
    }
}

/// Converte il file XPS `input` nel PDF `output`.
pub fn convert_file(input: &str, output: &str) -> Result<(), ConvertError> {
    let xps = fs::read(input)?;
    let pdf = convert_bytes(&xps)?;
    fs::write(output, pdf)?;
    Ok(())
}

/// Converte i byte di un pacchetto XPS in byte di un PDF.
pub fn convert_bytes(_xps: &[u8]) -> Result<Vec<u8>, ConvertError> {
    Err(ConvertError::Unsupported(
        "conversione non ancora implementata (fase 1 in corso)".into(),
    ))
}
