#![windows_subsystem = "windows"]

//! StampaCP: convertitore XPS -> PDF e finestra preferenze / anteprima nativa (egui).
//!
//! Uso da riga di comando:
//!   StampaCP <input.xps> <output.pdf>   -> Converte direttamente da CLI
//!   StampaCP <input.xps>                -> Apre la finestra nativa con anteprima e scelta stampante
//!   StampaCP                            -> Apre la finestra nativa di anteprima/configurazione

use eframe::egui;
use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

const VERSIONE: &str = "2.0.0";
const GITHUB: &str = "https://github.com/gsepp86/StampaCP";
const PAYPAL: &str = "https://paypal.me/PeppeCP";

/// Evento inviato dal sorvegliante della cartella di transito.
enum EventoCoda {
    Creato(String),
    Errore(String),
}

/// Sorvegliante attivo sulla cartella di transito.
struct CodaAttiva {
    cartella: String,
    attivo: Arc<AtomicBool>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum FormatoCarta {
    A4,
    A3,
    Letter,
}

impl FormatoCarta {
    fn label(&self) -> &'static str {
        match self {
            FormatoCarta::A4 => "A4 (210 × 297 mm)",
            FormatoCarta::A3 => "A3 (297 × 420 mm)",
            FormatoCarta::Letter => "Letter (216 × 279 mm)",
        }
    }

    fn dims_label(&self) -> &'static str {
        match self {
            FormatoCarta::A4 => "Carta: 210 × 297 mm",
            FormatoCarta::A3 => "Carta: 297 × 420 mm",
            FormatoCarta::Letter => "Carta: 216 × 279 mm",
        }
    }

    fn aspect_ratio(&self) -> f32 {
        match self {
            FormatoCarta::A4 => 210.0 / 297.0,
            FormatoCarta::A3 => 297.0 / 420.0,
            FormatoCarta::Letter => 216.0 / 279.0,
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum FronteRetro {
    SoloFronte,
    LatoLungo,
    LatoCorto,
}

impl FronteRetro {
    fn label(&self) -> &'static str {
        match self {
            FronteRetro::SoloFronte => "Solo fronte",
            FronteRetro::LatoLungo => "Fronte/retro (lato lungo)",
            FronteRetro::LatoCorto => "Fronte/retro (lato corto)",
        }
    }

    fn xml_val(&self) -> &'static str {
        match self {
            FronteRetro::SoloFronte => "none",
            FronteRetro::LatoLungo => "long",
            FronteRetro::LatoCorto => "short",
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum IntervalloPagine {
    Tutto,
    Corrente,
    Personalizzato,
}

#[derive(Clone, Copy, PartialEq)]
enum ModalitaScala {
    Adatta,
    Reale,
    Personalizzata,
}

#[derive(Clone, Copy, PartialEq)]
enum PosizioneNumero {
    BassoCentro,
    BassoDestra,
    BassoSinistra,
    AltoDestra,
}

impl PosizioneNumero {
    fn label(&self) -> &'static str {
        match self {
            PosizioneNumero::BassoCentro => "In basso al centro",
            PosizioneNumero::BassoDestra => "In basso a destra",
            PosizioneNumero::BassoSinistra => "In basso a sinistra",
            PosizioneNumero::AltoDestra => "In alto a destra",
        }
    }

    fn xml_val(&self) -> &'static str {
        match self {
            PosizioneNumero::BassoCentro => "bottom-center",
            PosizioneNumero::BassoDestra => "bottom-right",
            PosizioneNumero::BassoSinistra => "bottom-left",
            PosizioneNumero::AltoDestra => "top-right",
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum ModalitaColore {
    Colori,
    ScalaGrigi,
    BiancoNero,
}

impl ModalitaColore {
    fn xml_val(&self) -> &'static str {
        match self {
            ModalitaColore::Colori => "color",
            ModalitaColore::ScalaGrigi => "gray",
            ModalitaColore::BiancoNero => "bw",
        }
    }
}

struct StampaCPApp {
    input_xps: Option<String>,
    stampanti_fisiche: Vec<String>,
    destinazione_idx: usize, // 0 = Salva come PDF, 1.. = Stampanti fisiche
    cartella_pdf: String,
    nome_file_pdf: String,

    copie: u32,
    fascicola: bool,
    formato: FormatoCarta,
    fronte_retro: FronteRetro,

    intervallo: IntervalloPagine,
    intervallo_testo: String,

    pagine_per_foglio: u8,
    bordo_pagine: bool,
    scala: ModalitaScala,
    scala_percentuale: u32,
    ruota_auto: bool,
    centra_auto: bool,

    numera_pagine: bool,
    posizione_numero: PosizioneNumero,
    inizia_da: u32,

    colore: ModalitaColore,
    luminosita: i32,
    contrasto: i32,

    mostra_info: bool,
    mostra_avanzate: bool,
    messaggio_stato: Option<String>,
    aggiornamento_rx: Option<Receiver<String>>,

    cartella_coda: String,
    coda_attiva: bool,
    coda_rx: Option<Receiver<EventoCoda>>,
    coda_thread: Option<CodaAttiva>,
    ultimo_pdf: Option<String>,
}

impl StampaCPApp {
    fn new(input_xps: Option<String>) -> Self {
        let mut stampanti = rileva_stampanti_windows();
        if stampanti.is_empty() {
            stampanti.push("SHARP B/N STANDARD".to_string());
            stampanti.push("Stampante di rete (ufficio)".to_string());
        }

        let cartella_pdf = format!(r"{}\Stampe", cartella_documenti());
        let _ = fs::create_dir_all(&cartella_pdf);
        let cartella_coda = format!(r"{}\StampaCP\Coda", cartella_dati_locali());
        let _ = fs::create_dir_all(&cartella_coda);

        let mut app = Self {
            input_xps,
            stampanti_fisiche: stampanti,
            destinazione_idx: 0,
            cartella_pdf,
            nome_file_pdf: "Documento_StampaCP.pdf".to_string(),

            copie: 1,
            fascicola: true,
            formato: FormatoCarta::A4,
            fronte_retro: FronteRetro::SoloFronte,

            intervallo: IntervalloPagine::Tutto,
            intervallo_testo: "1-2".to_string(),

            pagine_per_foglio: 1,
            bordo_pagine: true,
            scala: ModalitaScala::Adatta,
            scala_percentuale: 100,
            ruota_auto: true,
            centra_auto: true,

            numera_pagine: false,
            posizione_numero: PosizioneNumero::BassoCentro,
            inizia_da: 1,

            colore: ModalitaColore::ScalaGrigi,
            luminosita: 0,
            contrasto: 0,

            mostra_info: false,
            mostra_avanzate: false,
            messaggio_stato: None,
            aggiornamento_rx: None,

            cartella_coda,
            coda_attiva: true,
            coda_rx: None,
            coda_thread: None,
            ultimo_pdf: None,
        };

        // Un lavoro XPS ricevuto dalla stampante virtuale viene convertito
        // subito, senza costringere l'utente a premere «Salva PDF».
        app.converti_ingresso_iniziale();
        app
    }

    fn is_pdf(&self) -> bool {
        self.destinazione_idx == 0
    }

    fn destinazione_corrente_label(&self) -> String {
        if self.destinazione_idx == 0 {
            "Salva come PDF (testo vettoriale)".to_string()
        } else {
            // Solo il nome: l'etichetta è la più lunga della finestra, che si
            // allarga in base alla stampante selezionabile più lunga.
            self.stampanti_fisiche
                .get(self.destinazione_idx - 1)
                .cloned()
                .unwrap_or_default()
        }
    }

    /// Larghezza della finestra necessaria a mostrare per intero il nome della
    /// stampante più lunga nella tendina «Destinazione».
    fn larghezza_preferita(&self) -> f32 {
        let caratteri = self
            .stampanti_fisiche
            .iter()
            .map(|nome| nome.chars().count())
            .max()
            .unwrap_or(0)
            .max("Salva come PDF (testo vettoriale)".chars().count());
        let colonna = (caratteri as f32 * 7.4 + 90.0).clamp(300.0, 560.0);
        (colonna * 3.0 + 36.0).clamp(1000.0, 1500.0)
    }

    /// Converte il file XPS ricevuto all'avvio nella cartella PDF configurata.
    fn converti_ingresso_iniziale(&mut self) {
        let Some(ingresso) = self.input_xps.clone() else {
            return;
        };
        let uscita = percorso_pdf_automatico(&self.cartella_pdf, &ingresso);
        match xps2pdf::convert_file(&ingresso, &uscita) {
            Ok(()) => {
                self.ultimo_pdf = Some(uscita.clone());
                self.messaggio_stato = Some(format!("PDF creato: {uscita}"));
            }
            Err(errore) => {
                self.messaggio_stato = Some(format!("Conversione non riuscita: {errore}"));
            }
        }
    }

    /// Avvia o ferma il sorvegliante della cartella di transito secondo le
    /// impostazioni correnti.
    fn aggiorna_coda(&mut self) {
        let cartella = self.cartella_coda.trim().to_string();
        if !self.coda_attiva || cartella.is_empty() {
            self.ferma_coda();
            return;
        }
        if let Some(attiva) = &self.coda_thread {
            if attiva.cartella == cartella {
                return;
            }
        }
        self.ferma_coda();
        let uscita = self.cartella_pdf.trim().to_string();
        let attivo = Arc::new(AtomicBool::new(true));
        let (tx, rx) = std::sync::mpsc::channel();
        let flag = Arc::clone(&attivo);
        let cartella_thread = cartella.clone();
        thread::spawn(move || sorveglia_coda(cartella_thread, uscita, flag, tx));
        self.coda_rx = Some(rx);
        self.coda_thread = Some(CodaAttiva { cartella, attivo });
    }

    fn ferma_coda(&mut self) {
        if let Some(attiva) = self.coda_thread.take() {
            attiva.attivo.store(false, Ordering::Relaxed);
        }
        self.coda_rx = None;
    }

    fn genera_print_ticket_xml(&self) -> String {
        let range_val = match self.intervallo {
            IntervalloPagine::Tutto => "all",
            IntervalloPagine::Corrente => "current",
            IntervalloPagine::Personalizzato => self.intervallo_testo.as_str(),
        };
        let scale_val = match self.scala {
            ModalitaScala::Adatta => "fit".to_string(),
            ModalitaScala::Reale => "real".to_string(),
            ModalitaScala::Personalizzata => self.scala_percentuale.to_string(),
        };
        let duplex_val = if self.is_pdf() {
            "none"
        } else {
            self.fronte_retro.xml_val()
        };
        let (dest_code, out_path) = if self.is_pdf() {
            (
                "pdf".to_string(),
                format!(r"{}\{}", self.cartella_pdf, self.nome_file_pdf),
            )
        } else {
            let nome = self
                .stampanti_fisiche
                .get(self.destinazione_idx - 1)
                .cloned()
                .unwrap_or_default();
            (format!("phys{}", self.destinazione_idx), nome)
        };

        format!(
            "<PrintTicket>\n\
             \x20 <Copies>{}</Copies>\n\
             \x20 <Collate>{}</Collate>\n\
             \x20 <PageSize>{:?}</PageSize>\n\
             \x20 <Duplex>{}</Duplex>\n\
             \x20 <Color>{}</Color>\n\
             \x20 <Brightness>{}</Brightness>\n\
             \x20 <Contrast>{}</Contrast>\n\
             \x20 <Scale>{}</Scale>\n\
             \x20 <PagesPerSheet>{}</PagesPerSheet>\n\
             \x20 <Border>{}</Border>\n\
             \x20 <Rotate>{}</Rotate>\n\
             \x20 <Center>{}</Center>\n\
             \x20 <PageNumbers>{}</PageNumbers>\n\
             \x20 <PageNumberPosition>{}</PageNumberPosition>\n\
             \x20 <PageNumberStart>{}</PageNumberStart>\n\
             \x20 <PageRange>{}</PageRange>\n\
             \x20 <Destination>{}</Destination>\n\
             \x20 <OutputPath>{}</OutputPath>\n\
             </PrintTicket>",
            self.copie,
            self.fascicola,
            self.formato,
            duplex_val,
            self.colore.xml_val(),
            self.luminosita,
            self.contrasto,
            scale_val,
            self.pagine_per_foglio,
            self.bordo_pagine,
            self.ruota_auto,
            self.centra_auto,
            self.numera_pagine,
            self.posizione_numero.xml_val(),
            self.inizia_da,
            range_val,
            dest_code,
            out_path
        )
    }

    fn esegui_azione(&mut self) {
        if self.is_pdf() {
            let out = format!(
                r"{}\{}",
                self.cartella_pdf.trim_end_matches('\\'),
                self.nome_file_pdf
            );
            if let Some(ref xps) = self.input_xps {
                match xps2pdf::convert_file(xps, &out) {
                    Ok(()) => {
                        self.ultimo_pdf = Some(out.clone());
                        self.messaggio_stato = Some(format!("PDF salvato in: {out}"));
                    }
                    Err(e) => {
                        self.messaggio_stato = Some(format!("Conversione: {e}"));
                    }
                }
            } else {
                self.messaggio_stato =
                    Some(format!("Pronto al salvataggio PDF vettoriale in: {out}"));
            }
        } else if let Some(stampante) = self.stampanti_fisiche.get(self.destinazione_idx - 1) {
            self.messaggio_stato = Some(format!(
                "Lavoro inviato a '{}' ({} copie)",
                stampante, self.copie
            ));
        }
    }
}

/// Controlla l'ultima release pubblicata nel repository GitHub.
fn controlla_aggiornamenti() -> String {
    #[cfg(target_os = "windows")]
    {
        let script = r#"$ErrorActionPreference = 'Stop'; try { $r = Invoke-RestMethod -UseBasicParsing -Uri 'https://api.github.com/repos/gsepp86/StampaCP/releases/latest' -Headers @{ 'User-Agent' = 'StampaCP' }; Write-Output ('OK|' + $r.tag_name + '|' + $r.html_url) } catch { if ($_.Exception.Response -and [int]$_.Exception.Response.StatusCode -eq 404) { Write-Output 'NONE' } else { Write-Output ('ERROR|' + $_.Exception.Message); exit 1 } }"#;
        let output = match Command::new("powershell")
            .args(["-NoProfile", "-NonInteractive", "-Command", script])
            .output()
        {
            Ok(output) if output.status.success() => output,
            Ok(output) => {
                return format!(
                    "Impossibile controllare gli aggiornamenti: {}",
                    String::from_utf8_lossy(&output.stderr).trim()
                );
            }
            Err(error) => return format!("Avvio controllo aggiornamenti non riuscito: {error}"),
        };

        let risposta = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if risposta == "NONE" {
            return "Nessuna release pubblicata su GitHub al momento.".to_string();
        }
        let mut campi = risposta.splitn(3, '|');
        if campi.next() != Some("OK") {
            return format!("Risposta GitHub non valida: {risposta}");
        }
        let versione = campi.next().unwrap_or_default();
        let url = campi.next().unwrap_or_default();
        if versione.is_empty() || url.is_empty() {
            return "Informazioni sulla versione non disponibili.".to_string();
        }
        if versione_nuova(versione, VERSIONE) {
            let _ = Command::new("cmd").args(["/C", "start", "", url]).spawn();
            format!("Nuova versione {versione} disponibile: pagina di download aperta.")
        } else {
            format!("StampaCP è aggiornato (versione {VERSIONE}).")
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        "Il controllo aggiornamenti è disponibile nella versione Windows.".to_string()
    }
}

#[cfg(target_os = "windows")]
fn versione_nuova(disponibile: &str, corrente: &str) -> bool {
    fn numeri(versione: &str) -> Vec<u64> {
        versione
            .trim()
            .trim_start_matches('v')
            .split('.')
            .map(|parte| {
                parte
                    .split('-')
                    .next()
                    .unwrap_or(parte)
                    .parse()
                    .unwrap_or(0)
            })
            .collect()
    }
    let nuova = numeri(disponibile);
    let attuale = numeri(corrente);
    for indice in 0..nuova.len().max(attuale.len()) {
        let a = *nuova.get(indice).unwrap_or(&0);
        let b = *attuale.get(indice).unwrap_or(&0);
        if a != b {
            return a > b;
        }
    }
    false
}

/// Rileva le stampanti installate su Windows tramite PowerShell/WMI senza dipendenze pesanti.
fn rileva_stampanti_windows() -> Vec<String> {
    #[cfg(target_os = "windows")]
    {
        if let Ok(output) = Command::new("powershell")
            .args([
                "-NoProfile",
                "-Command",
                "Get-Printer | Select-Object -ExpandProperty Name",
            ])
            .output()
        {
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                let list: Vec<String> = stdout
                    .lines()
                    .map(|l| l.trim().to_string())
                    .filter(|l| !l.is_empty() && !l.eq_ignore_ascii_case("StampaCP"))
                    .collect();
                if !list.is_empty() {
                    return list;
                }
            }
        }
    }
    Vec::new()
}

/// Cartella Documenti dell'utente corrente.
fn cartella_documenti() -> String {
    if let Ok(profilo) = env::var("USERPROFILE") {
        return Path::new(&profilo)
            .join("Documents")
            .to_string_lossy()
            .to_string();
    }
    env::current_dir()
        .map(|cartella| cartella.to_string_lossy().to_string())
        .unwrap_or_else(|_| ".".to_string())
}

/// Cartella dati locali dell'applicazione.
fn cartella_dati_locali() -> String {
    if let Ok(appdata) = env::var("LOCALAPPDATA") {
        return appdata;
    }
    if let Ok(profilo) = env::var("USERPROFILE") {
        return Path::new(&profilo)
            .join("AppData")
            .join("Local")
            .to_string_lossy()
            .to_string();
    }
    env::temp_dir().to_string_lossy().to_string()
}

/// Nome PDF libero nella cartella di uscita, derivato dal file XPS ricevuto.
fn percorso_pdf_automatico(cartella: &str, sorgente: &str) -> String {
    let _ = fs::create_dir_all(cartella);
    let cartella = Path::new(cartella);
    let base = Path::new(sorgente)
        .file_stem()
        .and_then(|nome| nome.to_str())
        .filter(|nome| !nome.trim().is_empty())
        .unwrap_or("Documento");
    let mut candidato = cartella.join(format!("{base}.pdf"));
    let mut indice = 2;
    while candidato.exists() && indice < 1000 {
        candidato = cartella.join(format!("{base}-{indice}.pdf"));
        indice += 1;
    }
    candidato.to_string_lossy().to_string()
}

/// Converte in PDF ogni file XPS che compare nella cartella di transito.
fn sorveglia_coda(
    cartella: String,
    uscita: String,
    attivo: Arc<AtomicBool>,
    tx: Sender<EventoCoda>,
) {
    let cartella = PathBuf::from(cartella);
    let _ = fs::create_dir_all(&cartella);
    let mut dimensioni: HashMap<PathBuf, u64> = HashMap::new();
    let mut tentativi: HashMap<PathBuf, u32> = HashMap::new();

    while attivo.load(Ordering::Relaxed) {
        if let Ok(voci) = fs::read_dir(&cartella) {
            for voce in voci.flatten() {
                let percorso = voce.path();
                if !percorso.is_file() {
                    continue;
                }
                let estensione = percorso
                    .extension()
                    .and_then(|valore| valore.to_str())
                    .unwrap_or_default()
                    .to_ascii_lowercase();
                if estensione != "xps" && estensione != "oxps" {
                    continue;
                }
                let dimensione = voce.metadata().map(|meta| meta.len()).unwrap_or(0);
                if dimensione == 0 {
                    continue;
                }
                // Attende che il driver abbia finito di scrivere il file.
                if dimensioni.get(&percorso) != Some(&dimensione) {
                    dimensioni.insert(percorso.clone(), dimensione);
                    tentativi.insert(percorso.clone(), 0);
                    continue;
                }
                let sorgente = percorso.to_string_lossy().to_string();
                let destinazione = percorso_pdf_automatico(&uscita, &sorgente);
                match xps2pdf::convert_file(&sorgente, &destinazione) {
                    Ok(()) => {
                        let _ = fs::remove_file(&percorso);
                        dimensioni.remove(&percorso);
                        tentativi.remove(&percorso);
                        if tx.send(EventoCoda::Creato(destinazione)).is_err() {
                            return;
                        }
                    }
                    Err(errore) => {
                        let conteggio = tentativi.entry(percorso.clone()).or_insert(0);
                        *conteggio += 1;
                        if *conteggio >= 4 {
                            let scarti = cartella.join("Errori");
                            let _ = fs::create_dir_all(&scarti);
                            let nome = percorso
                                .file_name()
                                .map(|n| n.to_os_string())
                                .unwrap_or_default();
                            let _ = fs::rename(&percorso, scarti.join(nome));
                            dimensioni.remove(&percorso);
                            tentativi.remove(&percorso);
                            let dettaglio = format!("{}: {errore}", percorso.display());
                            if tx.send(EventoCoda::Errore(dettaglio)).is_err() {
                                return;
                            }
                        }
                    }
                }
            }
        }
        thread::sleep(Duration::from_millis(1200));
    }
}

/// Apre un file con l'applicazione associata dal sistema operativo.
fn apri_file(percorso: &str) {
    #[cfg(target_os = "windows")]
    {
        let _ = Command::new("cmd")
            .args(["/C", "start", "", percorso])
            .spawn();
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = Command::new("xdg-open").arg(percorso).spawn();
    }
}

/// Registra un errore di conversione nel log dell'applicazione.
fn registra_errore(messaggio: &str) {
    eprintln!("{messaggio}");
    use std::io::Write;
    let cartella = format!(r"{}\StampaCP", cartella_dati_locali());
    let _ = fs::create_dir_all(&cartella);
    if let Ok(mut file) = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(Path::new(&cartella).join("StampaCP.log"))
    {
        let _ = writeln!(file, "{messaggio}");
    }
}

impl eframe::App for StampaCPApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let accent = egui::Color32::from_rgb(42, 111, 214);
        let danger = egui::Color32::from_rgb(200, 65, 58);

        // Spaziature e controlli ridotti per sfruttare meglio lo spazio disponibile.
        let mut style = (*ctx.style()).clone();
        style.spacing.item_spacing = egui::vec2(5.0, 3.0);
        style.spacing.button_padding = egui::vec2(5.0, 2.0);
        style.spacing.interact_size = egui::vec2(16.0, 16.0);
        style.spacing.slider_width = 90.0;
        style
            .text_styles
            .insert(egui::TextStyle::Body, egui::FontId::proportional(13.0));
        style
            .text_styles
            .insert(egui::TextStyle::Button, egui::FontId::proportional(13.0));
        ctx.set_style(style);

        // Recupera in modo asincrono l'esito della ricerca aggiornamenti.
        if let Some(rx) = &self.aggiornamento_rx {
            if let Ok(messaggio) = rx.try_recv() {
                self.messaggio_stato = Some(messaggio);
                self.aggiornamento_rx = None;
            }
        }

        // Lavori XPS convertiti dal sorvegliante della cartella di transito.
        let mut eventi_coda = Vec::new();
        if let Some(rx) = &self.coda_rx {
            while let Ok(evento) = rx.try_recv() {
                eventi_coda.push(evento);
            }
        }
        for evento in eventi_coda {
            match evento {
                EventoCoda::Creato(percorso) => {
                    self.ultimo_pdf = Some(percorso.clone());
                    self.messaggio_stato = Some(format!("PDF creato: {percorso}"));
                }
                EventoCoda::Errore(dettaglio) => {
                    self.messaggio_stato = Some(format!("Lavoro non convertito: {dettaglio}"));
                }
            }
        }
        if self.coda_rx.is_some() {
            ctx.request_repaint_after(Duration::from_millis(800));
        }

        // La stessa icona di StampaCP è incorporata nell'eseguibile e nella finestra.
        egui::TopBottomPanel::top("top_bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("StampaCP")
                        .size(17.0)
                        .strong()
                        .color(accent),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .small_button("ℹ Info")
                        .on_hover_text("Informazioni su StampaCP")
                        .clicked()
                    {
                        self.mostra_info = !self.mostra_info;
                    }
                    let cerca = ui.add_enabled(
                        self.aggiornamento_rx.is_none(),
                        egui::Button::new("↻ Cerca aggiornamenti"),
                    );
                    if cerca.clicked() {
                        let (tx, rx) = std::sync::mpsc::channel();
                        self.aggiornamento_rx = Some(rx);
                        self.messaggio_stato = Some("Ricerca aggiornamenti in corso…".to_string());
                        thread::spawn(move || {
                            let _ = tx.send(controlla_aggiornamenti());
                        });
                    }
                });
            });
        });

        // Barra inferiore
        egui::TopBottomPanel::bottom("bottom_bar").show(ctx, |ui| {
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if ui.button("Avanzate…").clicked() {
                    self.mostra_avanzate = !self.mostra_avanzate;
                }
                if ui.button("↻ Aggiorna stampanti").clicked() {
                    let rilevate = rileva_stampanti_windows();
                    if !rilevate.is_empty() {
                        self.stampanti_fisiche = rilevate;
                        self.destinazione_idx = 0;
                        self.messaggio_stato = Some("Elenco stampanti aggiornato.".to_string());
                    } else {
                        self.messaggio_stato =
                            Some("Nessuna nuova stampante rilevata dal sistema.".to_string());
                    }
                }

                if let Some(percorso) = self.ultimo_pdf.clone() {
                    if ui.small_button("Apri PDF").clicked() {
                        apri_file(&percorso);
                    }
                }

                if let Some(ref msg) = self.messaggio_stato {
                    ui.label(egui::RichText::new(msg).color(accent).small());
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let btn_label = if self.is_pdf() {
                        "  Salva PDF  "
                    } else {
                        "  Inoltra a stampante  "
                    };
                    let primary_btn = egui::Button::new(
                        egui::RichText::new(btn_label)
                            .color(egui::Color32::WHITE)
                            .strong(),
                    )
                    .fill(accent);

                    if ui.add(primary_btn).clicked() {
                        self.esegui_azione();
                    }
                    if ui.button("Annulla").clicked() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                });
            });
            ui.add_space(4.0);
        });

        // Finestra modale Info
        if self.mostra_info {
            egui::Window::new("Informazioni su StampaCP")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::LEFT_TOP, [12.0, 42.0])
                .show(ctx, |ui| {
                    ui.strong(format!("Stampante Virtuale CP · versione {VERSIONE}"));
                    ui.add_space(4.0);
                    ui.label("Stampante virtuale ultra-leggera in Rust con testo sempre ricercabile nei PDF e inoltro rapido verso stampanti fisiche.");
                    ui.label("Creata da Peppe CP con l'aiuto della IA.");
                    ui.add_space(8.0);
                    ui.hyperlink_to("Repository GitHub (gsepp86/StampaCP)", GITHUB);
                    ui.hyperlink_to("Sostieni il progetto (PayPal)", PAYPAL);
                    ui.add_space(8.0);
                    if ui.button("Chiudi").clicked() {
                        self.mostra_info = false;
                    }
                });
        }

        // Finestra modale Avanzate (PrintTicket XML)
        if self.mostra_avanzate {
            egui::Window::new("PrintTicket generato dalle opzioni")
                .collapsible(false)
                .default_size([460.0, 360.0])
                .show(ctx, |ui| {
                    egui::ScrollArea::vertical()
                        .max_height(280.0)
                        .show(ui, |ui| {
                            let xml = self.genera_print_ticket_xml();
                            ui.add(
                                egui::TextEdit::multiline(&mut xml.as_str())
                                    .font(egui::TextStyle::Monospace)
                                    .desired_width(f32::INFINITY),
                            );
                        });
                    ui.add_space(6.0);
                    if ui.button("Chiudi").clicked() {
                        self.mostra_avanzate = false;
                    }
                });
        }

        // Corpo principale a 3 colonne
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.columns(3, |cols| {
                // COLONNA 1: Stampante, Destinazione (tendina stampante fisica), Intervallo
                cols[0].group(|ui| {
                    ui.set_width(ui.available_width());
                    ui.label(egui::RichText::new("Stampante").color(danger).strong());
                    ui.label("Driver: Stampante PDF virtuale (testo vettoriale)");
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        ui.label("Copie:");
                        if ui.button("−").clicked() && self.copie > 1 {
                            self.copie -= 1;
                        }
                        ui.add(egui::DragValue::new(&mut self.copie).range(1..=999));
                        if ui.button("+").clicked() && self.copie < 999 {
                            self.copie += 1;
                        }
                        ui.checkbox(&mut self.fascicola, "Fascicola");
                    });
                    ui.horizontal(|ui| {
                        ui.label("Formato carta:");
                        egui::ComboBox::from_id_salt("formato_carta")
                            .selected_text(self.formato.label())
                            .show_ui(ui, |ui| {
                                ui.selectable_value(
                                    &mut self.formato,
                                    FormatoCarta::A4,
                                    FormatoCarta::A4.label(),
                                );
                                ui.selectable_value(
                                    &mut self.formato,
                                    FormatoCarta::A3,
                                    FormatoCarta::A3.label(),
                                );
                                ui.selectable_value(
                                    &mut self.formato,
                                    FormatoCarta::Letter,
                                    FormatoCarta::Letter.label(),
                                );
                            });
                    });
                    // Fronte/retro disattivato quando si seleziona "Salva come PDF"
                    ui.add_enabled_ui(!self.is_pdf(), |ui| {
                        ui.horizontal(|ui| {
                            ui.label("Fronte/retro:");
                            egui::ComboBox::from_id_salt("fronte_retro")
                                .selected_text(self.fronte_retro.label())
                                .show_ui(ui, |ui| {
                                    ui.selectable_value(
                                        &mut self.fronte_retro,
                                        FronteRetro::SoloFronte,
                                        FronteRetro::SoloFronte.label(),
                                    );
                                    ui.selectable_value(
                                        &mut self.fronte_retro,
                                        FronteRetro::LatoLungo,
                                        FronteRetro::LatoLungo.label(),
                                    );
                                    ui.selectable_value(
                                        &mut self.fronte_retro,
                                        FronteRetro::LatoCorto,
                                        FronteRetro::LatoCorto.label(),
                                    );
                                });
                        });
                    });
                });

                cols[0].add_space(3.0);

                cols[0].group(|ui| {
                    ui.set_width(ui.available_width());
                    ui.label(
                        egui::RichText::new("Destinazione (PDF o Stampante fisica)")
                            .color(danger)
                            .strong(),
                    );
                    egui::ComboBox::from_id_salt("destinazione_select")
                        .width(ui.available_width() - 8.0)
                        .selected_text(self.destinazione_corrente_label())
                        .show_ui(ui, |ui| {
                            ui.selectable_value(
                                &mut self.destinazione_idx,
                                0,
                                "Salva come PDF (testo vettoriale)",
                            );
                            for (i, sp) in self.stampanti_fisiche.iter().enumerate() {
                                ui.selectable_value(&mut self.destinazione_idx, i + 1, sp.as_str());
                            }
                        });

                    ui.add_space(4.0);
                    ui.add_enabled_ui(self.is_pdf(), |ui| {
                        ui.horizontal(|ui| {
                            ui.label("Cartella:");
                            ui.text_edit_singleline(&mut self.cartella_pdf);
                        });
                        ui.horizontal(|ui| {
                            ui.label("Nome file:");
                            ui.text_edit_singleline(&mut self.nome_file_pdf);
                        });
                    });

                    let note = if self.is_pdf() {
                        "Il PDF viene creato con testo ricercabile nella cartella indicata."
                    } else {
                        "Il lavoro viene inoltrato alla coda della stampante fisica selezionata."
                    };
                    ui.label(egui::RichText::new(note).small().weak());

                    ui.add_space(4.0);
                    ui.separator();
                    ui.checkbox(&mut self.coda_attiva, "Converti i lavori XPS in arrivo");
                    ui.add_enabled_ui(self.coda_attiva, |ui| {
                        ui.horizontal(|ui| {
                            ui.label("In arrivo:");
                            ui.add(
                                egui::TextEdit::singleline(&mut self.cartella_coda)
                                    .desired_width(f32::INFINITY),
                            );
                        });
                        ui.label(
                            egui::RichText::new(
                                "Ogni file XPS copiato qui viene convertito in PDF e rimosso dalla coda.",
                            )
                            .small()
                            .weak(),
                        );
                    });
                });

                cols[0].add_space(6.0);

                cols[0].group(|ui| {
                    ui.set_width(ui.available_width());
                    ui.label(
                        egui::RichText::new("Intervallo pagine · 2 totali")
                            .color(danger)
                            .strong(),
                    );
                    ui.radio_value(&mut self.intervallo, IntervalloPagine::Tutto, "Tutto");
                    ui.radio_value(&mut self.intervallo, IntervalloPagine::Corrente, "Corrente");
                    ui.radio_value(
                        &mut self.intervallo,
                        IntervalloPagine::Personalizzato,
                        "Personalizzato",
                    );
                    ui.add_enabled(
                        self.intervallo == IntervalloPagine::Personalizzato,
                        egui::TextEdit::singleline(&mut self.intervallo_testo),
                    );
                });

                // COLONNA 2: Pagine per foglio, Numero pagina, Colore / Luminosità / Contrasto
                cols[1].group(|ui| {
                    ui.set_width(ui.available_width());
                    ui.label(
                        egui::RichText::new("Pagine per foglio e scala")
                            .color(danger)
                            .strong(),
                    );
                    ui.horizontal(|ui| {
                        ui.label("Pagine per foglio:");
                        egui::ComboBox::from_id_salt("pagine_foglio")
                            .selected_text(format!("{} su 1", self.pagine_per_foglio))
                            .show_ui(ui, |ui| {
                                ui.selectable_value(&mut self.pagine_per_foglio, 1, "1 su 1");
                                ui.selectable_value(&mut self.pagine_per_foglio, 2, "2 su 1");
                                ui.selectable_value(&mut self.pagine_per_foglio, 4, "4 su 1");
                            });
                    });
                    ui.add_enabled(
                        self.scala != ModalitaScala::Reale,
                        egui::Checkbox::new(&mut self.bordo_pagine, "Bordo attorno alle pagine"),
                    );
                    ui.radio_value(&mut self.scala, ModalitaScala::Adatta, "Adatta alla pagina");
                    ui.radio_value(
                        &mut self.scala,
                        ModalitaScala::Reale,
                        "Dimensioni reali (100%)",
                    );
                    ui.horizontal(|ui| {
                        ui.radio_value(
                            &mut self.scala,
                            ModalitaScala::Personalizzata,
                            "Proporzione:",
                        );
                        ui.add_enabled(
                            self.scala == ModalitaScala::Personalizzata,
                            egui::DragValue::new(&mut self.scala_percentuale)
                                .range(10..=400)
                                .suffix("%"),
                        );
                    });
                    ui.checkbox(&mut self.ruota_auto, "Ruota automaticamente");
                    ui.checkbox(&mut self.centra_auto, "Centra automaticamente");
                });

                cols[1].add_space(3.0);

                cols[1].group(|ui| {
                    ui.set_width(ui.available_width());
                    ui.label(egui::RichText::new("Numero di pagina").color(danger).strong());
                    ui.checkbox(&mut self.numera_pagine, "Numera le pagine");
                    ui.add_enabled_ui(self.numera_pagine, |ui| {
                        ui.horizontal(|ui| {
                            ui.label("Posizione:");
                            egui::ComboBox::from_id_salt("pos_num")
                                .selected_text(self.posizione_numero.label())
                                .show_ui(ui, |ui| {
                                    ui.selectable_value(
                                        &mut self.posizione_numero,
                                        PosizioneNumero::BassoCentro,
                                        PosizioneNumero::BassoCentro.label(),
                                    );
                                    ui.selectable_value(
                                        &mut self.posizione_numero,
                                        PosizioneNumero::BassoDestra,
                                        PosizioneNumero::BassoDestra.label(),
                                    );
                                    ui.selectable_value(
                                        &mut self.posizione_numero,
                                        PosizioneNumero::BassoSinistra,
                                        PosizioneNumero::BassoSinistra.label(),
                                    );
                                    ui.selectable_value(
                                        &mut self.posizione_numero,
                                        PosizioneNumero::AltoDestra,
                                        PosizioneNumero::AltoDestra.label(),
                                    );
                                });
                        });
                        ui.horizontal(|ui| {
                            ui.label("Inizia da:");
                            ui.add(egui::DragValue::new(&mut self.inizia_da).range(0..=9999));
                        });
                    });
                });

                cols[1].add_space(6.0);

                cols[1].group(|ui| {
                    ui.set_width(ui.available_width());
                    ui.label(
                        egui::RichText::new("Colore, luminosità e contrasto")
                            .color(danger)
                            .strong(),
                    );
                    ui.radio_value(&mut self.colore, ModalitaColore::Colori, "Colori");
                    ui.radio_value(
                        &mut self.colore,
                        ModalitaColore::ScalaGrigi,
                        "Scala di grigi",
                    );
                    ui.radio_value(
                        &mut self.colore,
                        ModalitaColore::BiancoNero,
                        "Bianco e nero",
                    );
                    ui.add(
                        egui::Slider::new(&mut self.luminosita, -100..=100).text("Luminosità"),
                    );
                    ui.add(
                        egui::Slider::new(&mut self.contrasto, -100..=100).text("Contrasto"),
                    );
                });

                // COLONNA 3: Anteprima foglio vettoriale
                cols[2].vertical(|ui| {
                    ui.label(egui::RichText::new(self.formato.dims_label()).small().weak());

                    let avail = ui.available_size();
                    let box_h = (avail.y - 18.0).max(190.0);
                    let (preview_rect, _) = ui.allocate_exact_size(
                        egui::vec2(avail.x, box_h),
                        egui::Sense::hover(),
                    );

                    // Sfondo area anteprima
                    ui.painter().rect_filled(
                        preview_rect,
                        4.0,
                        egui::Color32::from_rgb(235, 235, 235),
                    );
                    ui.painter().rect_stroke(
                        preview_rect,
                        4.0,
                        egui::Stroke::new(1.0_f32, egui::Color32::from_gray(195)),
                    );

                    // Calcolo dimensioni foglio proporzionate
                    let scale_factor = match self.scala {
                        ModalitaScala::Adatta => 0.86,
                        ModalitaScala::Reale => 0.94,
                        ModalitaScala::Personalizzata => {
                            (self.scala_percentuale as f32 / 100.0).clamp(0.4, 0.96)
                        }
                    };
                    let sheet_h =
                        (preview_rect.height() * scale_factor).min(preview_rect.height() - 20.0);
                    let sheet_w =
                        (sheet_h * self.formato.aspect_ratio()).min(preview_rect.width() - 20.0);
                    let sheet_rect = egui::Rect::from_center_size(
                        preview_rect.center(),
                        egui::vec2(sheet_w, sheet_h),
                    );

                    // Tinta foglio in base a colore/luminosità
                    let lum_offset = (self.luminosita as f32 * 0.25) as i32;
                    let base_white = (255 + lum_offset).clamp(180, 255) as u8;
                    let sheet_bg = egui::Color32::from_rgb(base_white, base_white, base_white);

                    ui.painter().rect_filled(sheet_rect, 2.0, sheet_bg);
                    if self.bordo_pagine {
                        ui.painter().rect_stroke(
                            sheet_rect,
                            2.0,
                            egui::Stroke::new(1.0_f32, egui::Color32::from_gray(160)),
                        );
                    }

                    let contrast_delta = (self.contrasto as f32 * 0.35) as i32;
                    let gray_title = (50 - contrast_delta).clamp(0, 140) as u8;
                    let gray_body = (85 - contrast_delta).clamp(0, 170) as u8;

                    let title_color = match self.colore {
                        ModalitaColore::Colori => accent,
                        ModalitaColore::ScalaGrigi => egui::Color32::from_gray(gray_title),
                        ModalitaColore::BiancoNero => egui::Color32::BLACK,
                    };
                    let text_color = match self.colore {
                        ModalitaColore::Colori => egui::Color32::from_gray(gray_body),
                        ModalitaColore::ScalaGrigi => egui::Color32::from_gray(gray_body),
                        ModalitaColore::BiancoNero => egui::Color32::BLACK,
                    };

                    let content_rect = sheet_rect.shrink(14.0);
                    ui.painter().text(
                        content_rect.left_top(),
                        egui::Align2::LEFT_TOP,
                        "Documento di esempio",
                        egui::FontId::proportional(13.0),
                        title_color,
                    );

                    let sample_body = "Il testo di questa pagina resta selezionabile e ricercabile nel PDF finale, perché il convertitore usa glifi vettoriali e non immagini.\n\nLorem ipsum dolor sit amet, consectetur adipiscing elit. Sed do eiusmod tempor incididunt ut labore et dolore magna aliqua.";
                    ui.painter().text(
                        content_rect.left_top() + egui::vec2(0.0, 22.0),
                        egui::Align2::LEFT_TOP,
                        sample_body,
                        egui::FontId::proportional(10.0),
                        text_color,
                    );

                    if self.numera_pagine {
                        let (pos, align) = match self.posizione_numero {
                            PosizioneNumero::BassoCentro => (
                                sheet_rect.center_bottom() - egui::vec2(0.0, 8.0),
                                egui::Align2::CENTER_BOTTOM,
                            ),
                            PosizioneNumero::BassoDestra => (
                                sheet_rect.right_bottom() - egui::vec2(10.0, 8.0),
                                egui::Align2::RIGHT_BOTTOM,
                            ),
                            PosizioneNumero::BassoSinistra => (
                                sheet_rect.left_bottom() + egui::vec2(10.0, -8.0),
                                egui::Align2::LEFT_BOTTOM,
                            ),
                            PosizioneNumero::AltoDestra => (
                                sheet_rect.right_top() + egui::vec2(-10.0, 8.0),
                                egui::Align2::RIGHT_TOP,
                            ),
                        };
                        ui.painter().text(
                            pos,
                            align,
                            self.inizia_da.to_string(),
                            egui::FontId::proportional(10.0),
                            text_color,
                        );
                    }

                    ui.label(
                        egui::RichText::new(format!(
                            "Foglio 1 di 1 · {} pag. per foglio",
                            self.pagine_per_foglio
                        ))
                        .small()
                        .weak(),
                    );
                });
            });
        });

        // Riallinea il sorvegliante alla cartella eventualmente modificata.
        self.aggiorna_coda();
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = env::args().collect();
    let silenzioso = args
        .iter()
        .skip(1)
        .any(|argomento| argomento == "--silenzioso" || argomento == "--silent");
    let posizionali: Vec<String> = args
        .iter()
        .skip(1)
        .filter(|argomento| !argomento.starts_with("--"))
        .cloned()
        .collect();

    if posizionali.len() >= 2 {
        return match xps2pdf::convert_file(&posizionali[0], &posizionali[1]) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                registra_errore(&format!("Conversione di {}: {e}", posizionali[0]));
                ExitCode::FAILURE
            }
        };
    }

    let input_xps = posizionali.first().cloned();

    // Stampa da stampante virtuale senza aprire la finestra.
    if silenzioso {
        let Some(ingresso) = input_xps else {
            registra_errore("Modalità silenziosa senza file XPS di ingresso.");
            return ExitCode::FAILURE;
        };
        let uscita =
            percorso_pdf_automatico(&format!(r"{}\Stampe", cartella_documenti()), &ingresso);
        return match xps2pdf::convert_file(&ingresso, &uscita) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                registra_errore(&format!("Conversione di {ingresso}: {e}"));
                ExitCode::FAILURE
            }
        };
    }

    let icon_image =
        image::load_from_memory(include_bytes!(concat!(env!("OUT_DIR"), "/stampacp.ico")))
            .expect("icona StampaCP non valida")
            .into_rgba8();
    let icon = egui::IconData {
        width: icon_image.width(),
        height: icon_image.height(),
        rgba: icon_image.into_raw(),
    };

    let app = StampaCPApp::new(input_xps);
    // La finestra parte già larga quanto serve al nome della stampante più lunga.
    let larghezza = app.larghezza_preferita();

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([larghezza, 660.0])
            .with_min_inner_size([980.0, 540.0])
            .with_title("StampaCP - Stampante Virtuale")
            .with_icon(std::sync::Arc::new(icon)),
        ..Default::default()
    };

    match eframe::run_native("StampaCP", options, Box::new(move |_cc| Ok(Box::new(app)))) {
        Ok(()) => ExitCode::SUCCESS,
        Err(_) => ExitCode::FAILURE,
    }
}
