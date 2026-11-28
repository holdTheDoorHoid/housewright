//! Label reading end to end on a synthetic rating plate (made for this
//! test; not a photo of a real product). Uses the bundled models in
//! `models/ocr/`.

use housewright_media::ocr::Ocr;

fn models() -> Ocr {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../models/ocr/");
    let det = std::fs::read(format!("{dir}text-detection.onnx")).expect("bundled model");
    let rec = std::fs::read(format!("{dir}text-recognition.onnx")).expect("bundled model");
    Ocr::new(det, rec).unwrap()
}

#[test]
fn reads_a_synthetic_rating_plate() {
    let ocr = models();
    let photo = include_bytes!("fixtures/rating-plate-synthetic.jpg");
    let t = std::time::Instant::now();
    let lines = ocr.read_lines(photo).unwrap();
    eprintln!("read in {:?}: {lines:#?}", t.elapsed());
    // OCR cannot always tell O from 0 (or Q from 0) on a plate; the app
    // shows what it read and the person corrects it. Compare with those
    // folded together.
    let fold = |s: &str| s.to_uppercase().replace(['O', 'Q'], "0");
    let all = fold(&lines.join("\n"));
    assert!(all.contains("RHEEM"), "{all}");
    assert!(all.contains(&fold("XG40T06EC36U1")), "{all}");
    assert!(all.contains(&fold("Q081912345")), "{all}");
    assert!(all.contains("WATER HEATER"), "{all}");
}
