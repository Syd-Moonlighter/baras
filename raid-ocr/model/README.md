# OCR recognition model

`text-recognition.rten` reads names and health values from SWTOR ops frames.
BARAS downloads and caches it on first use; it is not part of the installer.

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

BARAS only uses the recognition model. It finds the text regions itself, so the
ocrs detection model is not needed.

## Upstream

- [ocrs](https://github.com/robertknight/ocrs) and
  [ocrs-models](https://github.com/robertknight/ocrs-models), by Robert Knight
- [Model repository](https://huggingface.co/robertknight/ocrs)
- Training data: HierText (CC BY-SA 4.0) and synthetic data

## License

The model is not covered by BARAS's MIT license. It is licensed under
[CC BY-SA 4.0](https://creativecommons.org/licenses/by-sa/4.0/); the full text
is in [`LICENSE-CC-BY-SA-4.0.txt`](LICENSE-CC-BY-SA-4.0.txt).

Attribution: *ocrs text recognition model* by
[Robert Knight](https://huggingface.co/robertknight/ocrs), redistributed
unmodified under CC BY-SA 4.0.