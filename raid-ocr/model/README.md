# OCR models

`text-recognition.rten` reads names and health values from SWTOR ops frames, and
`text-detection.rten` narrows each crop to the columns that hold text before it
does. BARAS downloads and caches them on first use.

Downloads use the repository copy first and the original ocrs S3 file as a
fallback. Both must match the SHA-256 in
[`../src/engine.rs`](../src/engine.rs).

| | |
| --- | --- |
| File | `text-recognition.rten` |
| Size | 9,716,568 bytes |
| SHA-256 | `e484866d4cce403175bd8d00b128feb08ab42e208de30e42cd9889d8f1735a6e` |
| Format | [rten](https://github.com/robertknight/rten) |
| Source | `https://ocrs-models.s3-accelerate.amazonaws.com/text-recognition.rten` |

| | |
| --- | --- |
| File | `text-detection.rten` |
| Size | 2,510,284 bytes |
| SHA-256 | `f15cfb56bd02c4bf478a20343986504a1f01e1665c2b3a0ad66340f054b1b5ca` |
| Format | [rten](https://github.com/robertknight/rten) |
| Source | `https://ocrs-models.s3-accelerate.amazonaws.com/text-detection.rten` |

BARAS finds the bands itself rather than using detection for layout; the
detection model is used only for its per-pixel text mask.

## Upstream

- [ocrs](https://github.com/robertknight/ocrs) and
  [ocrs-models](https://github.com/robertknight/ocrs-models), by Robert Knight
- [Model repository](https://huggingface.co/robertknight/ocrs)
- Training data: HierText (CC BY-SA 4.0) and synthetic data

## License

The model is not covered by BARAS's MIT license. It is licensed under
[CC BY-SA 4.0](https://creativecommons.org/licenses/by-sa/4.0/); the full text
is in [`LICENSE-CC-BY-SA-4.0.txt`](LICENSE-CC-BY-SA-4.0.txt).

Attribution: *ocrs text recognition and text detection models* by
[Robert Knight](https://huggingface.co/robertknight/ocrs), redistributed
unmodified under CC BY-SA 4.0.