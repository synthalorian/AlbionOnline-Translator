use albion_translator_lib::translator::TranslationEngine;

#[tokio::test]
async fn probe_detect_and_translate_live_spanish() {
    let mut engine = TranslationEngine::new();
    let texts = [
        "¡La Orden de la Cosecha Dorada recluta! Gremio nuevo, ambiente chill y sin obligaciones. Sin horarios ni requisitos, Susurra para mas info.",
        "busco heal, tank, flami, badon para grupales 7.2 en facc fort",
        "NOX GUILDA 4FUN(SEM TAX)COM MAPA E HO T7(ACEITAMOS NOVATOS/VETERANOS)-DG AVALON,DG GRUPO, ZVZ",
        "Ill take two fish for 4 ore",
    ];
    for t in texts {
        let det = engine.detect_language(t);
        eprintln!("DETECT {:?} -> {:?}", t, det);
        let out = engine.translate(t, det.as_deref()).await;
        eprintln!("TRANSLATE -> {:?}", out);
    }
}
