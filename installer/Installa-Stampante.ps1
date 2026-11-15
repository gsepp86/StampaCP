# Installa la stampante virtuale «StampaCP».
#
# La stampante usa il driver XPS incluso in Windows e consegna ogni lavoro
# nella cartella di transito sorvegliata da StampaCP, che lo converte in PDF
# con testo vettoriale e ricercabile.
#
# La coda sta in ProgramData (condivisa da tutti gli utenti) così funziona
# anche quando l'installazione viene eseguita con un altro account
# amministratore.
#
# Esecuzione manuale (da PowerShell come amministratore):
#   powershell -NoProfile -ExecutionPolicy Bypass -File Installa-Stampante.ps1
#
# Rimozione:
#   Remove-Printer -Name StampaCP
#   Remove-PrinterPort -Name "$env:ProgramData\StampaCP\Coda\documento.xps"

$ErrorActionPreference = 'Stop'

$coda = Join-Path $env:ProgramData 'StampaCP\Coda'
New-Item -ItemType Directory -Force -Path $coda | Out-Null
# Consente a tutti gli utenti di consegnare e rimuovere i lavori in coda.
icacls $coda /grant '*S-1-5-32-545:(OI)(CI)M' | Out-Null
$porta = Join-Path $coda 'documento.xps'

$driver = (Get-PrinterDriver -ErrorAction SilentlyContinue |
    Where-Object { $_.Name -like '*XPS*Document*Writer*' } |
    Select-Object -First 1).Name
if (-not $driver) {
    Write-Error "Driver 'Microsoft XPS Document Writer' non trovato: aggiungi il componente XPS di Windows (Impostazioni > App > Funzionalità facoltative) e riprova."
    exit 1
}

if (-not (Get-PrinterPort -Name $porta -ErrorAction SilentlyContinue)) {
    Add-PrinterPort -Name $porta
}

$esistente = Get-Printer -Name 'StampaCP' -ErrorAction SilentlyContinue
if ($esistente) {
    if ($esistente.PortName -ne $porta) {
        Set-Printer -Name 'StampaCP' -PortName $porta
    }
} else {
    Add-Printer -Name 'StampaCP' -DriverName $driver -PortName $porta
}

Write-Output "Stampante 'StampaCP' installata (driver: $driver)."
Write-Output "Cartella di transito: $coda"
exit 0
