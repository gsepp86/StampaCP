use base64::Engine as _;
use std::{env, fs, path::PathBuf};

fn main() {
    let manifest = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let encoded = fs::read_to_string(manifest.join("../assets/stampacp.ico.b64")).unwrap();
    let icon = base64::engine::general_purpose::STANDARD
        .decode(encoded.trim())
        .expect("icona ICO non valida");
    let icon_path = out.join("stampacp.ico");
    fs::write(&icon_path, icon).expect("impossibile scrivere l'icona");

    println!("cargo:rerun-if-changed=../assets/stampacp.ico.b64");

    #[cfg(windows)]
    {
        let mut resource = winres::WindowsResource::new();
        resource.set_icon(icon_path.to_str().expect("percorso icona non valido"));
        resource
            .compile()
            .expect("impossibile incorporare l'icona in StampaCP.exe");
    }
}
