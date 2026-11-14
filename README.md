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


## Configurazione e Installazione su Windows

Poiché **StampaCP** sfrutta i driver di stampa nativi e firmati di Windows (evitando la necessità di certificati kernel EV costosi o modalità di test disattivate), la configurazione richiede di associare la stampante virtuale a un driver standard e di puntarla al nostro eseguibile tramite un Port Monitor o una cartella di transito.

### 1. Installazione tramite Installer (Consigliato)
1. Compila lo script Inno Setup per generare l'eseguibile di installazione:
   ```cmd
   ISCC.exe installer\StampaCP.iss
