use base64::Engine as _;
use image::ImageEncoder;
use std::{env, fs, path::PathBuf};

fn main() {
    let manifest = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let encoded = fs::read_to_string(manifest.join("../assets/stampacp.ico.b64")).unwrap();
    let source = base64::engine::general_purpose::STANDARD
        .decode(encoded.trim())
        .expect("icona ICO non valida");
    let original = image::load_from_memory(&source)
        .expect("impossibile leggere l'icona originale")
        .into_rgba8();

    // Produce una risorsa ICO a 256 px: Windows può ridimensionarla per il desktop
    // senza dover ingrandire al volo la vecchia risorsa da 32 px.
    let enlarged = image::imageops::resize(
        &original,
        256,
        256,
        image::imageops::FilterType::Lanczos3,
    );
    let sharpened = image::imageops::unsharpen(&enlarged, 1.2, 6);
    let icon_path = out.join("stampacp.ico");
    let mut icon_file = fs::File::create(&icon_path).expect("impossibile creare l'icona ICO");
    image::codecs::ico::IcoEncoder::new(&mut icon_file)
        .write_image(
            sharpened.as_raw(),
            256,
            256,
            image::ExtendedColorType::Rgba8,
        )
        .expect("impossibile generare l'icona ICO ad alta risoluzione");

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
