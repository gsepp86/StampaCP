# StampaCP 2.0 · Stampante virtuale in Rust per Windows

## Se vuoi contribuire al mio lavoro ti lascio in basso il pulsante per contribuire anche con una piccola donazione simbolica visto il tempo che ti farò risparmiare per stampare grossi quantitativi di pdf ;-)

[![Donate via PayPal](https://img.shields.io/badge/Donate-PayPal-blue.svg?logo=paypal)](https://paypal.me/PeppeCP)

Progetto di stampante PDF virtuale ultra-leggera che conserva il testo vettoriale ricercabile, con finestra nativa **egui / eframe** (zero dipendenze Chrome/WebView2) e inoltro rapido verso stampanti fisiche.

## La versione 2.0 include l’icona applicativa condivisa tra finestra, eseguibile e collegamento desktop, oltre al controllo delle nuove release pubblicate su GitHub. Il pulsante “Cerca aggiornamenti” apre la pagina della release quando trova una versione più recente.

## Struttura

| Percorso | Ruolo | Stato |
|---|---|---|
| `xps2pdf/src/main.rs` | Interfaccia grafica nativa (`egui`) con anteprima, PrintTicket, contrasto/luminosità e selettore stampante fisica | pronto |
| `xps2pdf/src/lib.rs` | Converte XPS in PDF con testo vettoriale | pronto |
| `installer/StampaCP.iss` | Installer Windows (Inno Setup 6) | pronto |
| `anteprima-psa.html` | Prototipo statico di riferimento della UI | riferimento |

## Uso da riga di comando

- `StampaCP.exe` — Apre la finestra nativa di anteprima e gestione stampa.
- `StampaCP.exe <input.xps>` — Converte subito il lavoro XPS in PDF e apre la finestra con l'esito.
- `StampaCP.exe <input.xps> --silenzioso` — Converte il lavoro in PDF e termina senza aprire la finestra (adatto a un Port Monitor).
- `StampaCP.exe <input.xps> <output.pdf>` — Converte direttamente da CLI senza interfaccia grafica.

## Stampante virtuale: come funziona la conversione

La pagina viene convertita in un PDF con testo vettoriale e ricercabile: percorsi, immagini e glifi
sono scritti come contenuto PDF, i font incorporati nel pacchetto XPS (compresi quelli offuscati
`.odttf`) vengono riutilizzati nel PDF e il testo resta selezionabile.

La finestra si allarga automaticamente in base al nome della stampante più lunga, così la tendina
«Destinazione» non tronca il nome selezionato.

Per usare StampaCP come stampante virtuale basta consegnarle il file XPS prodotto dal driver:

1. ogni file `.xps`/`.oxps` copiato nella cartella di transito (predefinita: `%LOCALAPPDATA%\StampaCP\Coda`)
   viene convertito in PDF nella cartella `Documenti\Stampe` e rimosso dalla coda; i lavori non
   convertibili dopo alcuni tentativi vengono spostati in `Coda\Errori`;
2. la stessa cartella e la cartella di uscita sono modificabili dal riquadro «Destinazione»
   (casella «Converti i lavori XPS in arrivo»);
3. in alternativa il driver può richiamare direttamente `StampaCP.exe <input.xps> --silenzioso`.


## Configurazione e Installazione su Windows

Poiché **StampaCP** sfrutta i driver di stampa nativi e firmati di Windows (evitando la necessità di certificati kernel EV costosi o modalità di test disattivate), la configurazione richiede di associare la stampante virtuale a un driver standard e di puntarla al nostro eseguibile tramite un Port Monitor o una cartella di transito.

### 1. Installazione tramite Installer (Consigliato)
1. Compila lo script Inno Setup per generare l'eseguibile di installazione:
   ```cmd
   ISCC.exe installer\StampaCP.iss
