# StampaCP · Stampante virtuale in Rust per Windows

Progetto di stampante PDF virtuale ultra-leggera che conserva il testo vettoriale ricercabile, con finestra nativa **egui / eframe** (zero dipendenze Chrome/WebView2) e inoltro rapido verso stampanti fisiche.

## Struttura

| Percorso | Ruolo | Stato |
|---|---|---|
| `xps2pdf/src/main.rs` | Interfaccia grafica nativa (`egui`) con anteprima, PrintTicket, contrasto/luminosità e selettore stampante fisica | pronto |
| `xps2pdf/src/lib.rs` | Converte XPS in PDF con testo vettoriale | scheletro (fase 1) |
| `installer/StampaCP.iss` | Installer Windows (Inno Setup 6) | pronto |
| `anteprima-psa.html` | Prototipo statico di riferimento della UI | riferimento |

## Uso da riga di comando

- `StampaCP.exe` — Apre la finestra nativa di anteprima e gestione stampa.
- `StampaCP.exe <input.xps>` — Apre la finestra nativa caricando il lavoro XPS intercettato.
- `StampaCP.exe <input.xps> <output.pdf>` — Converte direttamente da CLI senza interfaccia grafica.
