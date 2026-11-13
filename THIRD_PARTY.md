# Third-party source

1. `vendor/step-io`: https://github.com/elgar328/step-io, commit `2d7bd2a9d55fe9a4c8b12e3a8f39bcafe2575300`, MIT OR Apache-2.0. Linked as a path dependency. The generated schema model carries the upstream notices about ISO schema provenance; see its README and retained licences.
2. `vendor/step-loupe`: https://github.com/elgar328/step-loupe, MIT OR Apache-2.0. Commit `fabcde872f321215a10e6a5ff1ee97c3c506ce88`. Reference checkout only, not compiled or linked. Its bundled `sample/nist-ctc05.step` is used only for STEP reader smoke checks. Retained upstream licences and documentation.
3. Other Rust dependencies are resolved in `Cargo.lock` and retain their respective upstream licences. No third-party images or branding are used.
