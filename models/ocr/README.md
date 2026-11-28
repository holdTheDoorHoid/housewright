# OCR models (label reading, D-74)

`text-detection.onnx` and `text-recognition.onnx` are the pre-trained models
of the ocrs OCR engine, used to read appliance rating plates on the device.
They are bundled with the app and never downloaded at run time.

Attribution (as the ocrs-models project asks):

> Ocrs models (https://github.com/robertknight/ocrs-models), trained on the
> HierText dataset by Google Research. Licensed under CC BY-SA 4.0.

- Licence: Creative Commons Attribution-ShareAlike 4.0 International
  (https://creativecommons.org/licenses/by-sa/4.0/). The models inherit it
  from the HierText dataset (https://github.com/google-research-datasets/hiertext).
- Source: the ocrs project's published copies,
  `https://ocrs-models.s3-accelerate.amazonaws.com/text-detection.onnx` and
  `.../text-recognition.onnx`, downloaded 2026-11-27. The ocrs project says
  those URLs may not last and that apps should bundle their own copy, which
  is why they are in this repository.
- Unmodified. If you change them, the changed models must also be CC BY-SA 4.0.

| File | Bytes | SHA-256 |
|---|---|---|
| text-detection.onnx | 2,499,479 | a917b23dbd9524b465df7e922641b3ff2981623df4ded5a0234004ef2fee7cfe |
| text-recognition.onnx | 9,713,177 | 86c145c2edb96c8caed5b1ebb8f44d706408922211451c309c625157dd6061c5 |
