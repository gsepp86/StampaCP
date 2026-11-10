//! StampaCP: convertitore XPS -> PDF da riga di comando.
//!
//! Uso: StampaCP <input.xps> <output.pdf>
//!
//! Stato: scheletro. Il parsing del pacchetto XPS (ZIP + FixedPage)
//! e la scrittura del PDF con testo vettoriale sono da implementare.

use std::env;
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = env::args().collect();
    if args.len() != 3 {
        eprintln!("Uso: StampaCP <input.xps> <output.pdf>");
        return ExitCode::from(2);
    }

    match xps2pdf::convert_file(&args[1], &args[2]) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("Errore: {e}");
            ExitCode::FAILURE
        }
    }
}
