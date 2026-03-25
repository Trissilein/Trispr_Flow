fn main() {
    // NOTE: comctl32 v6 manifest is now embedded automatically by tauri-winres
    // via the generated resource.rc (RT_MANIFEST / type 24). Manual embedding
    // via /MANIFESTINPUT+/MANIFEST:EMBED would cause CVT1100 duplicate resource.
    tauri_build::build()
}
