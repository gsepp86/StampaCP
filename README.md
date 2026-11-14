# StampaCP 2.0 · Stampante virtuale in Rust per Windows

## Se vuoi contribuire al mio lavoro ti lascio in basso il pulsante per contribuire anche con una piccola donazione simbolica visto il tempo che ti farò risparmiare per stampare grossi quantitativi di pdf ;-)

[![Donate via PayPal](https://img.shields.io/badge/Donate-PayPal-blue.svg?logo=paypal)](https://paypal.me/PeppeCP)

Progetto di stampante PDF virtuale ultra-leggera che conserva il testo vettoriale ricercabile, con finestra nativa **egui / eframe** (zero dipendenze Chrome/WebView2) e inoltro rapido verso stampanti fisiche.

## La versione 2.0 include l’icona applicativa condivisa tra finestra, eseguibile e collegamento desktop, oltre al controllo delle nuove release pubblicate su GitHub. Il pulsante “Cerca aggiornamenti” apre la pagina della release quando trova una versione più recente.

## Struttura

| Percorso | Ruolo | Stato |
|---|---|---|
| `xps2pdf/src/main.rs` | Interfaccia grafica nativa (`egui`) con anteprima, PrintTicket, contrasto/luminosità e selettore stampante fisica | pronto |
| `xps2pdf/src/lib.rs` | Conversione XPS→PDF | non implementata |
| `installer/StampaCP.iss` | Installer Windows (Inno Setup 6), solo GUI | parziale: non installa la stampante virtuale |
| `anteprima-psa.html` | Prototipo statico di riferimento della UI | riferimento |

## Stato attuale: prototipo, non installare la stampante virtuale

**Questa versione non è ancora pronta per l'installazione o per l'uso quotidiano come stampante Windows.** L'installer Inno Setup presente nel repository copia soltanto `StampaCP.exe`: non registra il Port Monitor, non crea una porta e non crea una coda di stampa. La DLL nativa è un prototipo di sviluppo e non deve essere registrata manualmente nello spooler di Windows.

Prima di abilitare l'installazione servono almeno:
- definire e verificare una combinazione driver/print path supportata da Windows 10 LTSC x64;
- completare la registrazione e la rimozione della porta, con gestione persistente e corretta delle code;
- implementare la lettura del pacchetto XPS e la conversione XPS→PDF;
- sostituire l'anteprima dimostrativa con il rendering del documento ricevuto;
- eseguire test end-to-end su una macchina Windows dedicata, inclusi installazione, stampa, riavvio dello spooler e disinstallazione.

**Non creare manualmente una coda che punti a questa DLL e non copiare la DLL nelle cartelle di sistema.** I test correnti di CI verificano compilazione e test automatici, non certificano il funzionamento della pipeline di stampa reale.

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
